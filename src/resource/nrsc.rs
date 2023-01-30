use core::mem::size_of;
use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use miniz_oxide::inflate::core as zlib;

use crate::{abi_utils::TransmuteSafe, resource::decompress, Error};

#[derive(Debug, Clone)]
pub(crate) struct NrscIndex {
    idx: Vec<NrscIdxRecord>,
    ids: String, // contains null bytes as substring separators
}

mod abi {

    use super::Format;
    use crate::Error;

    // TODO: Use LE16 & LE32?
    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub(crate) struct NrscIdxRecord {
        format: u16,
        fileseq: u16,
        id_str_offset: u32,
        file_offset: u32,
        len: u32,
    }

    impl NrscIdxRecord {
        pub fn id_str_offset(&self) -> usize {
            u32::from_le(self.id_str_offset) as usize
        }

        pub(super) fn format(&self) -> Result<Format, Error> {
            match u16::from_le(self.format) {
                0 => Ok(Format::Uncompressed),
                1 => Ok(Format::Zlib),
                _ => Err(Error::InvalidAudioFormat),
            }
        }

        pub fn fileseq(&self) -> usize {
            u16::from_le(self.fileseq) as usize
        }

        pub fn file_offset(&self) -> u64 {
            u32::from_le(self.file_offset) as u64
        }

        pub fn len(&self) -> usize {
            u32::from_le(self.len) as usize
        }
    }

    #[test]
    fn test_audio_index() {
        use super::NrscIndex;
        use std::mem::size_of;
        let air = |id_str_offset| NrscIdxRecord {
            format: 0,
            fileseq: 0,
            id_str_offset,
            file_offset: 0,
            len: 0,
        };
        let mut audio_idx = NrscIndex {
            idx: vec![air(0), air(1), air(3), air(6), air(10)],
            ids: "\0a\0bb\0ccc\0dddd".to_owned(),
        };

        let diff = 8 + audio_idx.idx.len() * size_of::<NrscIdxRecord>();
        // Fix offsets now that they are known
        for air in audio_idx.idx.iter_mut() {
            air.id_str_offset += diff as u32;
        }

        assert_eq!(audio_idx.get_id_at(diff + 0).unwrap(), "");
        assert_eq!(audio_idx.get_id_at(diff + 1).unwrap(), "a");
        assert_eq!(audio_idx.get_id_at(diff + 3).unwrap(), "bb");
        assert_eq!(audio_idx.get_id_at(diff + 4), Err(Error::InvalidIndex));
        assert_eq!(audio_idx.get_id_at(diff + 6).unwrap(), "ccc");
        assert_eq!(audio_idx.get_id_at(diff + 10), Err(Error::InvalidIndex));

        audio_idx.ids = "\0a\0bb\0ccc\0dddd\0".to_owned();
        let diff = diff as u32;
        assert_eq!(audio_idx.get_by_id("").unwrap(), air(diff + 0));
        assert_eq!(audio_idx.get_by_id("a").unwrap(), air(diff + 1));
        assert_eq!(audio_idx.get_by_id("bb").unwrap(), air(diff + 3));
        assert_eq!(audio_idx.get_by_id("ccc").unwrap(), air(diff + 6));
        assert_eq!(audio_idx.get_by_id("dddd").unwrap(), air(diff + 10));
        assert_eq!(audio_idx.get_by_id("ddd"), Err(Error::NotFound));
    }
}

pub(crate) use abi::NrscIdxRecord;

use super::ResourceFile;

enum Format {
    Uncompressed,
    Zlib,
}

unsafe impl TransmuteSafe for NrscIdxRecord {}

impl NrscIndex {
    pub(crate) fn new(path: &Path) -> Result<Self, Error> {
        let path = path.join("index.nidx");
        let mut file = File::open(path).map_err(|_| Error::FopenError)?;
        let mut len = [0; 8];
        file.read_exact(&mut len).map_err(|_| Error::IOError)?;
        let len = u32::from_le_bytes(len[4..8].try_into().unwrap()) as usize;
        let file_size = file.metadata().map_err(|_| Error::IOError)?.len() as usize;
        let idx_expected_size = size_of::<NrscIdxRecord>() * len + 8;
        let mut idx = vec![NrscIdxRecord::default(); len];
        let mut ids = String::with_capacity(file_size - idx_expected_size);
        file.read_exact(NrscIdxRecord::slice_as_bytes_mut(idx.as_mut_slice()))
            .map_err(|_| Error::IOError)?;
        file.read_to_string(&mut ids).map_err(|_| Error::IOError)?;
        Ok(Self { idx, ids })
    }

    fn get_id_at(&self, offset: usize) -> Result<&str, Error> {
        let offset = offset - (size_of::<NrscIdxRecord>() * self.idx.len() + 8);
        if offset > 0 && &self.ids[offset - 1..offset] != "\0" {
            return Err(Error::InvalidIndex);
        }
        let tail = &self.ids[offset..];
        let len = tail.find('\0').ok_or(Error::InvalidIndex)?;
        Ok(&tail[..len])
    }

    pub fn get_by_id(&self, id: &str) -> Result<NrscIdxRecord, Error> {
        let mut idx_err = Ok(());
        let i = self
            .idx
            .binary_search_by_key(&id, |idx| match self.get_id_at(idx.id_str_offset()) {
                Ok(ok) => ok,
                Err(err) => {
                    idx_err = Err(err);
                    ""
                }
            })
            .map_err(|_| Error::NotFound)?;
        idx_err?;

        Ok(self.idx[i])
    }

    pub fn get_by_idx(&self, idx: usize) -> Result<(&str, NrscIdxRecord), Error> {
        let idx_rec = self.idx.get(idx).copied().ok_or(Error::InvalidIndex)?;
        let item_id = self.get_id_at(idx_rec.id_str_offset())?;
        Ok((item_id, idx_rec))
    }
}

pub struct Nrsc {
    index: NrscIndex,
    data: NrscData,
}

struct NrscData {
    files: Vec<ResourceFile>,
    read_buf: Vec<u8>,
    decomp_buf: Vec<u8>,
    zlib_state: zlib::DecompressorOxide,
}

impl Nrsc {
    fn parse_fname(fname: &OsStr) -> Option<u32> {
        let fname = fname.to_str()?;
        if fname.ends_with(".nrsc") {
            let secnum_end = fname.len() - ".nrsc".len();
            u32::from_str_radix(&fname[..secnum_end], 10).ok()
        } else {
            None
        }
    }

    fn files(path: &Path) -> Result<Vec<ResourceFile>, Error> {
        let mut files = Vec::new();

        for entry in fs::read_dir(path).map_err(|_| Error::IOError)? {
            let entry = entry.map_err(|_| Error::IOError)?;
            let seqnum = Nrsc::parse_fname(&entry.file_name());
            if let Some(seqnum) = seqnum {
                files.push(ResourceFile {
                    seqnum,
                    len: entry.metadata().map_err(|_| Error::IOError)?.len() as usize,
                    offset: 0,
                    file: File::open(entry.path()).map_err(|_| Error::IOError)?,
                });
            }
        }
        let mut offset = 0;
        files.sort_by_key(|f| f.seqnum);
        for (i, cf) in files.iter_mut().enumerate() {
            if cf.seqnum != i as u32 {
                return Err(Error::MissingResourceFile);
            }
            cf.offset = offset;
            offset += cf.len;
        }
        Ok(files)
    }

    pub(crate) fn new(path: &Path) -> Result<Self, Error> {
        let files = Nrsc::files(path)?;
        let index = NrscIndex::new(path)?;
        Ok(Nrsc {
            index,
            data: NrscData {
                files,
                read_buf: Vec::new(),
                decomp_buf: Vec::new(),
                zlib_state: zlib::DecompressorOxide::new(),
            },
        })
    }

    pub fn get_by_idx(&mut self, idx: usize) -> Result<(&str, &[u8]), Error> {
        let (id, nidx_rec) = self.index.get_by_idx(idx)?;
        let item = self.data.get_by_nidx_rec(nidx_rec)?;
        Ok((id, item))
    }

    pub fn get(&mut self, id: &str) -> Result<&[u8], Error> {
        self.data.get_by_nidx_rec(self.index.get_by_id(id)?)
    }

    pub fn len(&self) -> usize {
        self.index.idx.len()
    }
}

impl NrscData {
    fn get_by_nidx_rec(&mut self, idx: NrscIdxRecord) -> Result<&[u8], Error> {
        let file = &mut self.files[idx.fileseq() as usize];

        file.file
            .seek(SeekFrom::Start(idx.file_offset()))
            .map_err(|_| Error::IOError)?;
        if self.read_buf.len() < idx.len() {
            self.read_buf.resize(idx.len(), 0);
        }
        file.file
            .read_exact(&mut self.read_buf[..idx.len()])
            .map_err(|_| Error::IOError)?;

        match idx.format()? {
            Format::Uncompressed => Ok(&self.read_buf[..idx.len()]),
            Format::Zlib => {
                let n_out = decompress(
                    &mut self.zlib_state,
                    &self.read_buf[..idx.len()],
                    &mut self.decomp_buf,
                )?;
                Ok(&self.decomp_buf[..n_out])
            }
        }
    }

}
