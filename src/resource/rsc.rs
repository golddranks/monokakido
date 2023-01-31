use core::{cmp::min, mem::size_of, ops::Not, slice};
use miniz_oxide::inflate::core as zlib;
use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use crate::{
    abi_utils::{TransmuteSafe, LE32},
    resource::decompress,
    Error,
};

mod abi {
    use crate::abi_utils::LE32;

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub(crate) struct IdxRecord {
        pub item_id: LE32,
        pub map_idx: LE32,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
    pub struct MapRecord {
        pub(crate) zoffset: LE32,
        pub(crate) ioffset: LE32,
    }

    #[test]
    fn test_get_by_id() {
        use super::RscIndex;
        use crate::Error;

        fn idx(id: u32, idx: u32) -> IdxRecord {
            IdxRecord {
                item_id: id.into(),
                map_idx: idx.into(),
            }
        }
        fn map(z: u32, i: u32) -> MapRecord {
            MapRecord {
                zoffset: z.into(),
                ioffset: i.into(),
            }
        }

        assert_eq!(
            RscIndex {
                idx: Some(vec![]),
                map: vec![],
            }
            .get_by_id(500),
            Err(Error::NotFound)
        );

        assert_eq!(
            RscIndex {
                idx: Some(vec![idx(1, 0)]),
                map: vec![map(0, 0)],
            }
            .get_by_id(500),
            Err(Error::NotFound)
        );

        assert_eq!(
            RscIndex {
                idx: Some(vec![idx(1, 0), idx(2, 1)]),
                map: vec![map(0, 0), map(0, 10)],
            }
            .get_by_id(500),
            Err(Error::NotFound)
        );

        assert_eq!(
            RscIndex {
                idx: Some(vec![idx(1, 0), idx(2, 1), idx(1000, 2)]),
                map: vec![map(0, 0), map(0, 10), map(0, 20)],
            }
            .get_by_id(500),
            Err(Error::NotFound)
        );

        assert_eq!(
            RscIndex {
                idx: Some(vec![idx(1, 0), idx(2, 1), idx(500, 2), idx(1000, 3)]),
                map: vec![map(0, 0), map(0, 10), map(0, 20), map(10, 0)],
            }
            .get_by_id(500),
            Ok(map(0, 20))
        );

        assert_eq!(
            RscIndex {
                idx: Some(vec![
                    idx(1, 0),
                    idx(2, 1),
                    idx(499, 2),
                    idx(500, 3),
                    idx(501, 4),
                    idx(1000, 5)
                ]),
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
pub(crate) use abi::{IdxRecord, MapRecord};

use super::ResourceFile;

#[derive(Debug, Clone)]
pub(crate) struct RscIndex {
    idx: Option<Vec<IdxRecord>>,
    map: Vec<MapRecord>,
}

unsafe impl TransmuteSafe for MapRecord {}
unsafe impl TransmuteSafe for IdxRecord {}

impl RscIndex {
    fn load_idx(path: &Path) -> Result<Option<Vec<IdxRecord>>, Error> {
        let path = path.with_extension("idx");
        if path.exists().not() {
            return Ok(None);
        };
        let mut idx_file = File::open(path)?;
        let mut len = [0; 4];
        idx_file.read_exact(&mut len)?;
        let len = u32::from_le_bytes(len) as usize;
        idx_file.seek(SeekFrom::Start(8))?;
        let idx_size = idx_file.metadata().map_err(|_| Error::IOError)?.len();
        let idx_expected_size = (size_of::<IdxRecord>() * len + 8) as u64;
        if idx_size != idx_expected_size {
            return Err(Error::IncorrectStreamLength);
        }
        let mut idx = vec![IdxRecord::default(); len];
        idx_file
            .read_exact(IdxRecord::slice_as_bytes_mut(idx.as_mut_slice()))
            .map_err(|_| Error::IOError)?;
        Ok(Some(idx))
    }

    fn load_map(path: &Path) -> Result<Vec<MapRecord>, Error> {
        let path = path.with_extension("map");
        let mut map_file = File::open(path)?;
        let mut len = [0; 4];
        map_file.seek(SeekFrom::Start(4))?;
        map_file.read_exact(&mut len)?;
        let len = u32::from_le_bytes(len) as usize;
        map_file.seek(SeekFrom::Start(8))?;
        let map_size = map_file.metadata().map_err(|_| Error::IOError)?.len();
        let map_expected_size = (size_of::<MapRecord>() * len + 8) as u64;
        if map_size != map_expected_size {
            return Err(Error::IncorrectStreamLength);
        }
        let mut map = vec![MapRecord::default(); len];
        map_file
            .read_exact(MapRecord::slice_as_bytes_mut(map.as_mut_slice()))
            .map_err(|_| Error::IOError)?;
        Ok(map)
    }
    pub(crate) fn new(path: &Path, rsc_name: &str) -> Result<Self, Error> {
        let path = path.join(rsc_name); // filename stem
        let idx = Self::load_idx(&path)?;
        let map = Self::load_map(&path)?;
        Ok(RscIndex { idx, map })
    }

    fn get_map_idx_by_id(&self, id: u32) -> Result<usize, Error> {
        let Some(idx_list) = &self.idx else {
            return Ok(id as usize);
        };
        if idx_list.is_empty() {
            return Err(Error::NotFound);
        }

        // Let's guess first, since usually the IDs are completely predictable, without gaps.
        let idx = min(id as usize, idx_list.len() - 1);
        let guess = idx_list[idx].item_id.read();
        if id == guess {
            return Ok(idx);
        }
        let idx = min(id.saturating_sub(1) as usize, idx_list.len() - 1);
        let guess = idx_list[idx].item_id.read();
        if id == guess {
            return Ok(idx);
        }
        let map_idx = idx_list
            .binary_search_by_key(&id, |r| r.item_id.read())
            .map(|idx| idx_list[idx].map_idx.us())
            .map_err(|_| Error::NotFound)?;
        if map_idx >= self.map.len() {
            return Err(Error::IndexMismach);
        }
        Ok(map_idx)
    }

    pub fn get_by_id(&self, id: u32) -> Result<MapRecord, Error> {
        let idx = self.get_map_idx_by_id(id)?;
        let record = self.map[idx];
        Ok(record)
    }

    pub fn get_by_idx(&self, idx: usize) -> Result<(u32, MapRecord), Error> {
        let item_id = if let Some(indexes) = &self.idx {
            let idx_rec = indexes.get(idx).copied().ok_or(Error::InvalidIndex)?;
            if idx_rec.map_idx.us() != idx {
                return Err(Error::InvalidIndex);
            };
            idx_rec.item_id.read()
        } else {
            idx as u32
        };
        let map_rec = self.map.get(idx).copied().ok_or(Error::InvalidIndex)?;
        Ok((item_id, map_rec))
    }
}

pub struct Rsc {
    index: RscIndex,
    files: Vec<ResourceFile>,
    zlib_buf: Vec<u8>,
    zlib_state: zlib::DecompressorOxide,
    contents_buf: Vec<u8>,
    current_offset: usize,
    current_len: usize,
}

impl Rsc {
    fn parse_fname(rsc_name: &str, fname: &OsStr) -> Option<u32> {
        let fname = fname.to_str()?;
        let ext = ".rsc";
        let min_len = rsc_name.len() + 1 + ext.len();
        if fname.starts_with(rsc_name) && fname.ends_with(ext) && fname.len() > min_len {
            let seqnum_start = rsc_name.len() + 1;
            let seqnum_end = fname.len() - ext.len();
            fname[seqnum_start..seqnum_end].parse().ok()
        } else {
            None
        }
    }

    fn files(path: &Path, rsc_name: &str) -> Result<Vec<ResourceFile>, Error> {
        let mut files = Vec::new();

        for entry in fs::read_dir(path).map_err(|_| Error::IOError)? {
            let entry = entry.map_err(|_| Error::IOError)?;
            let seqnum = Self::parse_fname(rsc_name, &entry.file_name());
            if let Some(seqnum) = seqnum {
                files.push(ResourceFile {
                    seqnum,
                    len: entry.metadata().map_err(|_| Error::IOError)?.len() as usize,
                    offset: 0,
                    file: File::open(entry.path()).map_err(|_| Error::IOError)?,
                });
            }
        }
        files.sort_by_key(|f| f.seqnum);
        let mut offset = 0;
        for (i, cf) in files.iter_mut().enumerate() {
            if cf.seqnum != i as u32 + 1 {
                return Err(Error::MissingResourceFile);
            }
            cf.offset = offset;
            offset += cf.len;
        }
        Ok(files)
    }

    pub(crate) fn new(path: &Path, rsc_name: &str) -> Result<Self, Error> {
        let files = Rsc::files(path, rsc_name)?;
        let index = RscIndex::new(path, rsc_name)?;
        Ok(Self {
            index,
            files,
            zlib_buf: Vec::new(),
            zlib_state: zlib::DecompressorOxide::new(),
            contents_buf: Vec::new(),
            current_offset: 0,
            current_len: 0,
        })
    }

    fn load_contents(&mut self, zoffset: usize) -> Result<(), Error> {
        let (file, file_offset) = file_offset(&mut self.files, zoffset)?;

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

    pub fn get(&mut self, id: u32) -> Result<&[u8], Error> {
        self.get_by_map(self.index.get_by_id(id)?)
    }

    pub fn get_by_idx(&mut self, idx: usize) -> Result<(u32, &[u8]), Error> {
        let (id, map_rec) = self.index.get_by_idx(idx)?;
        let item = self.get_by_map(map_rec)?;
        Ok((id, item))
    }

    fn get_by_map(&mut self, idx: MapRecord) -> Result<&[u8], Error> {
        if self.contents_buf.is_empty() || idx.zoffset.us() != self.current_offset {
            self.load_contents(idx.zoffset.us())?;
        }

        let contents = &self.contents_buf[idx.ioffset.us()..self.current_len];
        let (len, contents_tail) = LE32::from(contents)?;
        Ok(&contents_tail[..len.us()])
    }

    pub fn len(&self) -> usize {
        self.index.map.len()
    }
}

fn file_offset(contents: &mut [ResourceFile], offset: usize) -> Result<(&mut File, u64), Error> {
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
    let one_file = &mut vec![ResourceFile {
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
        ResourceFile {
            seqnum: 1,
            len: 100,
            offset: 0,
            file: f1,
        },
        ResourceFile {
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
        ResourceFile {
            seqnum: 1,
            len: 100,
            offset: 0,
            file: f1,
        },
        ResourceFile {
            seqnum: 2,
            len: 200,
            offset: 100,
            file: f2,
        },
        ResourceFile {
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

pub struct RscIter<'a> {
    map: slice::Iter<'a, MapRecord>,
}

impl<'a> Iterator for RscIter<'a> {
    type Item = MapRecord;

    fn next(&mut self) -> Option<Self::Item> {
        self.map.next().copied()
    }
}
