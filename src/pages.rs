use core::{cmp::min, mem::size_of, ops::Not};
use miniz_oxide::inflate::core as zlib;
use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
};

use crate::{
    abi::{TransmuteSafe, LE32},
    decompress,
    dict::Paths,
    ContentsFile, Error,
};

mod abi {
    use crate::abi::LE32;

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub(crate) struct TextIdxRecord {
        pub dic_item_id: LE32,
        pub map_idx: LE32,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
    pub(crate) struct TextMapRecord {
        pub zoffset: LE32,
        pub ioffset: LE32,
    }

    #[test]
    fn test_get_by_id() {
        use crate::{pages::PageIndex, Error};

        fn idx(id: u32, idx: u32) -> TextIdxRecord {
            TextIdxRecord {
                dic_item_id: id.into(),
                map_idx: idx.into(),
            }
        }
        fn map(z: u32, i: u32) -> TextMapRecord {
            TextMapRecord {
                zoffset: z.into(),
                ioffset: i.into(),
            }
        }

        assert_eq!(
            PageIndex {
                idx: vec![],
                map: vec![],
            }
            .get_by_id(500),
            Err(Error::NotFound)
        );

        assert_eq!(
            PageIndex {
                idx: vec![idx(1, 0)],
                map: vec![map(0, 0)],
            }
            .get_by_id(500),
            Err(Error::NotFound)
        );

        assert_eq!(
            PageIndex {
                idx: vec![idx(1, 0), idx(2, 1)],
                map: vec![map(0, 0), map(0, 10)],
            }
            .get_by_id(500),
            Err(Error::NotFound)
        );

        assert_eq!(
            PageIndex {
                idx: vec![idx(1, 0), idx(2, 1), idx(1000, 2)],
                map: vec![map(0, 0), map(0, 10), map(0, 20)],
            }
            .get_by_id(500),
            Err(Error::NotFound)
        );

        assert_eq!(
            PageIndex {
                idx: vec![idx(1, 0), idx(2, 1), idx(500, 2), idx(1000, 3)],
                map: vec![map(0, 0), map(0, 10), map(0, 20), map(10, 0)],
            }
            .get_by_id(500),
            Ok(map(0, 20))
        );

        assert_eq!(
            PageIndex {
                idx: vec![
                    idx(1, 0),
                    idx(2, 1),
                    idx(499, 2),
                    idx(500, 3),
                    idx(501, 4),
                    idx(1000, 5)
                ],
                map: vec![
                    map(0, 0),
                    map(0, 10),
                    map(0, 20),
                    map(10, 0),
                    map(10, 0),
                    map(10, 0)
                ],
            }
            .get_by_id(500),
            Ok(map(10, 0))
        );
    }
}
pub(crate) use abi::{TextIdxRecord, TextMapRecord};

#[derive(Debug, Clone)]
pub(crate) struct PageIndex {
    idx: Vec<TextIdxRecord>,
    map: Vec<TextMapRecord>,
}

unsafe impl TransmuteSafe for TextMapRecord {}
unsafe impl TransmuteSafe for TextIdxRecord {}

impl PageIndex {
    pub(crate) fn new(paths: &Paths) -> Result<Self, Error> {
        let mut idx_file = File::open(paths.contents_idx_path())?;
        let mut map_file = File::open(paths.contents_map_path())?;
        let mut len = [0; 4];
        idx_file.read_exact(&mut len)?;
        let len = u32::from_le_bytes(len) as usize;
        idx_file.seek(SeekFrom::Start(8))?;
        map_file.seek(SeekFrom::Start(8))?;
        let idx_size = idx_file.metadata().map_err(|_| Error::IOError)?.len();
        let map_size = map_file.metadata().map_err(|_| Error::IOError)?.len();
        let idx_expected_size = (size_of::<TextIdxRecord>() * len + 8) as u64;
        let map_expected_size = (size_of::<TextMapRecord>() * len + 8) as u64;
        if idx_size != idx_expected_size || map_size != map_expected_size {
            return Err(Error::IncorrectStreamLength);
        }
        let mut idx = vec![TextIdxRecord::default(); len];
        let mut map = vec![TextMapRecord::default(); len];
        idx_file
            .read_exact(TextIdxRecord::slice_as_bytes_mut(idx.as_mut_slice()))
            .map_err(|_| Error::IOError)?;
        map_file
            .read_exact(TextMapRecord::slice_as_bytes_mut(map.as_mut_slice()))
            .map_err(|_| Error::IOError)?;
        Ok(PageIndex { idx, map })
    }

    fn get_idx_by_id(&self, id: u32) -> Option<usize> {
        if self.idx.is_empty() {
            return None;
        }
        // Let's guess first, since usually the IDs are completely predictable, without gaps.
        let idx_list = self.idx.as_slice();
        let idx = min(id as usize, idx_list.len() - 1);
        let guess = idx_list[idx].dic_item_id.read();
        if id == guess {
            return Some(idx);
        }
        let idx = min(id.saturating_sub(1) as usize, idx_list.len() - 1);
        let guess = idx_list[idx].dic_item_id.read();
        if id == guess {
            return Some(idx);
        }
        return idx_list
            .binary_search_by_key(&id, |r| r.dic_item_id.read())
            .ok();
    }

    pub fn get_by_id(&self, id: u32) -> Result<TextMapRecord, Error> {
        if let Some(idx) = self.get_idx_by_id(id) {
            let record = self.map[self.idx[idx].map_idx.us()];
            Ok(record)
        } else {
            Err(Error::NotFound)
        }
    }
}

pub struct Pages {
    index: PageIndex,
    contents: Vec<ContentsFile>,
    zlib_buf: Vec<u8>,
    zlib_state: zlib::DecompressorOxide,
    contents_buf: Vec<u8>,
    current_offset: usize,
    current_len: usize,
}

impl Pages {
    fn parse_fname(fname: &OsStr) -> Option<u32> {
        let fname = fname.to_str()?;
        if (fname.starts_with("contents-") && fname.ends_with(".rsc")).not() {
            return None;
        }
        u32::from_str_radix(&fname[9..13], 10).ok()
    }

    pub(crate) fn new(paths: &Paths) -> Result<Self, Error> {
        let mut contents = Vec::new();
        for entry in fs::read_dir(&paths.contents_path()).map_err(|_| Error::IOError)? {
            let entry = entry.map_err(|_| Error::IOError)?;
            let seqnum = Pages::parse_fname(&entry.file_name());
            if let Some(seqnum) = seqnum {
                contents.push(ContentsFile {
                    seqnum,
                    len: entry.metadata().map_err(|_| Error::IOError)?.len() as usize,
                    offset: 0,
                    file: File::open(entry.path()).map_err(|_| Error::IOError)?,
                });
            }
        }
        contents.sort_by_key(|f| f.seqnum);
        let mut offset = 0;
        for (i, cf) in contents.iter_mut().enumerate() {
            if cf.seqnum != i as u32 + 1 {
                return Err(Error::NoContentFilesFound);
            }
            cf.offset = offset;
            offset += cf.len;
        }
        let index = PageIndex::new(&paths)?;
        Ok(Pages {
            index,
            contents,
            zlib_buf: Vec::new(),
            zlib_state: zlib::DecompressorOxide::new(),
            contents_buf: Vec::new(),
            current_offset: 0,
            current_len: 0,
        })
    }

    fn load_contents(&mut self, zoffset: usize) -> Result<(), Error> {
        let (file, file_offset) = file_offset(&mut self.contents, zoffset)?;

        let mut len = [0_u8; 4];
        file.seek(SeekFrom::Start(file_offset))
            .map_err(|_| Error::IOError)?;
        file.read_exact(&mut len).map_err(|_| Error::IOError)?;
        let len = u32::from_le_bytes(len) as usize;
        if self.zlib_buf.len() < len {
            self.zlib_buf.resize(len, 0);
        }
        file.read_exact(&mut self.zlib_buf[..len])
            .map_err(|_| Error::IOError)?;

        let n_out = decompress(
            &mut self.zlib_state,
            &self.zlib_buf[..len],
            &mut self.contents_buf,
        )?;

        self.current_len = n_out;
        self.current_offset = zoffset;

        Ok(())
    }

    pub fn get(&mut self, id: u32) -> Result<&str, Error> {
        self.get_by_idx(self.index.get_by_id(id)?)
    }

    fn get_by_idx(&mut self, idx: TextMapRecord) -> Result<&str, Error> {
        if self.contents_buf.is_empty() || idx.zoffset.us() != self.current_offset {
            self.load_contents(idx.zoffset.us())?;
        }

        let contents = &self.contents_buf[idx.ioffset.us()..self.current_len];
        let (len, contents_tail) = LE32::from(contents)?;
        Ok(std::str::from_utf8(&contents_tail[..len.us()]).map_err(|_| Error::Utf8Error)?)
    }
}

fn file_offset(contents: &mut [ContentsFile], offset: usize) -> Result<(&mut File, u64), Error> {
    let file_idx = contents
        .binary_search_by(|cf| cmp_range(offset, cf.offset..cf.offset + cf.len).reverse())
        .map_err(|_| Error::InvalidIndex)?;
    let cf = &mut contents[file_idx];
    let file = &mut cf.file;
    let file_offset = (offset - cf.offset) as u64;
    Ok((file, file_offset))
}

#[test]
fn test_file_offset() {
    use std::os::unix::prelude::AsRawFd;

    assert_eq!(file_offset(&mut [], 0).err(), Some(Error::InvalidIndex));

    let mock_file = || {
        let f = File::open("/dev/zero").unwrap();
        let fd = f.as_raw_fd();
        (f, fd)
    };
    let (f1, f1_fd) = mock_file();
    let one_file = &mut vec![ContentsFile {
        seqnum: 1,
        len: 100,
        offset: 0,
        file: f1,
    }];

    let result = file_offset(one_file, 101);
    assert_eq!(result.err(), Some(Error::InvalidIndex));

    let result = file_offset(one_file, 100);
    assert_eq!(result.err(), Some(Error::InvalidIndex));

    let result = file_offset(one_file, 0);
    assert_eq!(result.as_ref().map(|f| f.0.as_raw_fd()), Ok(f1_fd));
    assert_eq!(result.as_ref().map(|f| f.1), Ok(0));

    let result = file_offset(one_file, 99);
    assert_eq!(result.as_ref().map(|f| f.0.as_raw_fd()), Ok(f1_fd));
    assert_eq!(result.as_ref().map(|f| f.1), Ok(99));

    let (f1, f1_fd) = mock_file();
    let (f2, f2_fd) = mock_file();
    let two_files = &mut vec![
        ContentsFile {
            seqnum: 1,
            len: 100,
            offset: 0,
            file: f1,
        },
        ContentsFile {
            seqnum: 2,
            len: 200,
            offset: 100,
            file: f2,
        },
    ];

    let result = file_offset(two_files, 301);
    assert_eq!(result.err(), Some(Error::InvalidIndex));

    let result = file_offset(two_files, 300);
    assert_eq!(result.err(), Some(Error::InvalidIndex));

    let result = file_offset(two_files, 0);
    assert_eq!(result.as_ref().map(|f| f.0.as_raw_fd()), Ok(f1_fd));
    assert_eq!(result.as_ref().map(|f| f.1), Ok(0));

    let result = file_offset(two_files, 99);
    assert_eq!(result.as_ref().map(|f| f.0.as_raw_fd()), Ok(f1_fd));
    assert_eq!(result.as_ref().map(|f| f.1), Ok(99));

    let result = file_offset(two_files, 100);
    assert_eq!(result.as_ref().map(|f| f.0.as_raw_fd()), Ok(f2_fd));
    assert_eq!(result.as_ref().map(|f| f.1), Ok(0));

    let result = file_offset(two_files, 299);
    assert_eq!(result.as_ref().map(|f| f.0.as_raw_fd()), Ok(f2_fd));
    assert_eq!(result.as_ref().map(|f| f.1), Ok(199));

    let (f1, f1_fd) = mock_file();
    let (f2, f2_fd) = mock_file();
    let (f3, f3_fd) = mock_file();
    let three_files = &mut vec![
        ContentsFile {
            seqnum: 1,
            len: 100,
            offset: 0,
            file: f1,
        },
        ContentsFile {
            seqnum: 2,
            len: 200,
            offset: 100,
            file: f2,
        },
        ContentsFile {
            seqnum: 3,
            len: 100,
            offset: 300,
            file: f3,
        },
    ];

    let result = file_offset(three_files, 401);
    assert_eq!(result.err(), Some(Error::InvalidIndex));

    let result = file_offset(three_files, 400);
    assert_eq!(result.err(), Some(Error::InvalidIndex));

    let result = file_offset(three_files, 0);
    assert_eq!(result.as_ref().map(|f| f.0.as_raw_fd()), Ok(f1_fd));
    assert_eq!(result.as_ref().map(|f| f.1), Ok(0));

    let result = file_offset(three_files, 99);
    assert_eq!(result.as_ref().map(|f| f.0.as_raw_fd()), Ok(f1_fd));
    assert_eq!(result.as_ref().map(|f| f.1), Ok(99));

    let result = file_offset(three_files, 100);
    assert_eq!(result.as_ref().map(|f| f.0.as_raw_fd()), Ok(f2_fd));
    assert_eq!(result.as_ref().map(|f| f.1), Ok(0));

    let result = file_offset(three_files, 299);
    assert_eq!(result.as_ref().map(|f| f.0.as_raw_fd()), Ok(f2_fd));
    assert_eq!(result.as_ref().map(|f| f.1), Ok(199));

    let result = file_offset(three_files, 300);
    assert_eq!(result.as_ref().map(|f| f.0.as_raw_fd()), Ok(f3_fd));
    assert_eq!(result.as_ref().map(|f| f.1), Ok(0));

    let result = file_offset(three_files, 399);
    assert_eq!(result.as_ref().map(|f| f.0.as_raw_fd()), Ok(f3_fd));
    assert_eq!(result.as_ref().map(|f| f.1), Ok(99));
}

fn cmp_range(num: usize, range: core::ops::Range<usize>) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    if num < range.start {
        Ordering::Less
    } else if range.end <= num {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

#[test]
fn test_cmp_to_range() {
    use core::cmp::Ordering;
    assert_eq!(cmp_range(0, 0..0), Ordering::Greater);
    assert_eq!(cmp_range(0, 0..1), Ordering::Equal);
    assert_eq!(cmp_range(0, 0..100), Ordering::Equal);
    assert_eq!(cmp_range(1, 0..100), Ordering::Equal);
    assert_eq!(cmp_range(99, 0..100), Ordering::Equal);
    assert_eq!(cmp_range(100, 0..100), Ordering::Greater);
    assert_eq!(cmp_range(101, 0..100), Ordering::Greater);
    assert_eq!(cmp_range(0, 1..100), Ordering::Less);
    assert_eq!(cmp_range(99, 100..100), Ordering::Less);
    assert_eq!(cmp_range(100, 100..100), Ordering::Greater);
}
