use core::{mem::size_of, ops::Not};
use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
};

use miniz_oxide::inflate::core as zlib;

use crate::{abi::TransmuteSafe, decompress, dict::Paths, ContentsFile, Error};

#[derive(Debug, Clone)]
pub(crate) struct AudioIndex {
    idx: Vec<AudioIdxRecord>,
    ids: String, // contains null bytes as substring separators
}

mod abi {
    use crate::{audio::AudioFormat, Error};

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub(crate) struct AudioIdxRecord {
        format: u16,
        fileseq: u16,
        id_str_offset: u32,
        file_offset: u32,
        len: u32,
    }

    impl AudioIdxRecord {
        pub fn id_str_offset(&self) -> usize {
            u32::from_le(self.id_str_offset) as usize
        }

        pub(super) fn format(&self) -> Result<AudioFormat, Error> {
            match u16::from_le(self.format) {
                0 => Ok(AudioFormat::Acc),
                1 => Ok(AudioFormat::ZlibAcc),
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
        use crate::audio::AudioIndex;
        let air = |id_str_offset| AudioIdxRecord {
            format: 0,
            fileseq: 0,
            id_str_offset,
            file_offset: 0,
            len: 0,
        };
        let mut audio_idx = AudioIndex {
            idx: vec![air(0), air(1), air(3), air(6), air(10)],
            ids: "\0a\0bb\0ccc\0dddd".to_owned(),
        };
        assert_eq!(audio_idx.get_id_at(0).unwrap(), "");
        assert_eq!(audio_idx.get_id_at(1).unwrap(), "a");
        assert_eq!(audio_idx.get_id_at(3).unwrap(), "bb");
        assert_eq!(audio_idx.get_id_at(4), Err(Error::InvalidIndex));
        assert_eq!(audio_idx.get_id_at(6).unwrap(), "ccc");
        assert_eq!(audio_idx.get_id_at(10), Err(Error::InvalidIndex));

        audio_idx.ids = "\0a\0bb\0ccc\0dddd\0".to_owned();
        assert_eq!(audio_idx.get_by_id("").unwrap(), air(0));
        assert_eq!(audio_idx.get_by_id("a").unwrap(), air(1));
        assert_eq!(audio_idx.get_by_id("bb").unwrap(), air(3));
        assert_eq!(audio_idx.get_by_id("ccc").unwrap(), air(6));
        assert_eq!(audio_idx.get_by_id("dddd").unwrap(), air(10));
        assert_eq!(audio_idx.get_by_id("ddd"), Err(Error::NotFound));
    }
}

pub(crate) use abi::AudioIdxRecord;

enum AudioFormat {
    Acc,
    ZlibAcc,
}

unsafe impl TransmuteSafe for AudioIdxRecord {}

impl AudioIndex {
    pub(crate) fn new(paths: &Paths) -> Result<Self, Error> {
        let mut file = File::open(paths.audio_idx_path()).map_err(|_| Error::FopenError)?;
        let mut len = [0; 8];
        file.read_exact(&mut len).map_err(|_| Error::IOError)?;
        let len = u32::from_le_bytes(len[4..8].try_into().unwrap()) as usize;
        let file_size = file.metadata().map_err(|_| Error::IOError)?.len() as usize;
        let idx_expected_size = size_of::<AudioIdxRecord>() * len + 8;
        let mut idx = vec![AudioIdxRecord::default(); len];
        let mut ids = String::with_capacity(file_size - idx_expected_size);
        file.read_exact(AudioIdxRecord::slice_as_bytes_mut(idx.as_mut_slice()))
            .map_err(|_| Error::IOError)?;
        file.read_to_string(&mut ids).map_err(|_| Error::IOError)?;
        Ok(Self { idx, ids })
    }

    fn get_id_at(&self, offset: usize) -> Result<&str, Error> {
        let offset = offset - (size_of::<AudioIdxRecord>() * self.idx.len() + 8);
        if offset > 0 && &self.ids[offset - 1..offset] != "\0" {
            return Err(Error::InvalidIndex);
        }
        let tail = &self.ids[offset..];
        let len = tail.find('\0').ok_or(Error::InvalidIndex)?;
        Ok(&tail[..len])
    }

    pub fn get_by_id(&self, id: &str) -> Result<AudioIdxRecord, Error> {
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
}

pub struct Audio {
    index: AudioIndex,
    audio: Vec<ContentsFile>,
    read_buf: Vec<u8>,
    decomp_buf: Vec<u8>,
    zlib_state: zlib::DecompressorOxide,
}

impl Audio {
    fn parse_fname(fname: &OsStr) -> Option<u32> {
        let fname = fname.to_str()?;
        if fname.ends_with(".nrsc").not() {
            return None;
        }
        u32::from_str_radix(&fname[..5], 10).ok()
    }

    pub(crate) fn new(paths: &Paths) -> Result<Self, Error> {
        let mut audio = Vec::new();
        for entry in fs::read_dir(&paths.audio_path()).map_err(|_| Error::IOError)? {
            let entry = entry.map_err(|_| Error::IOError)?;
            let seqnum = Audio::parse_fname(&entry.file_name());
            if let Some(seqnum) = seqnum {
                audio.push(ContentsFile {
                    seqnum,
                    len: entry.metadata().map_err(|_| Error::IOError)?.len() as usize,
                    offset: 0,
                    file: File::open(entry.path()).map_err(|_| Error::IOError)?,
                });
            }
        }
        audio.sort_by_key(|f| f.seqnum);
        if Some(audio.len()) != audio.last().map(|a| a.seqnum as usize + 1) {
            return Err(Error::NoContentFilesFound);
        }
        let index = AudioIndex::new(&paths)?;
        Ok(Audio {
            index,
            audio,
            read_buf: Vec::new(),
            decomp_buf: Vec::new(),
            zlib_state: zlib::DecompressorOxide::new(),
        })
    }

    fn get_by_idx(&mut self, idx: AudioIdxRecord) -> Result<&[u8], Error> {
        let file = &mut self.audio[idx.fileseq() as usize];

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
            AudioFormat::Acc => Ok(&self.read_buf[..idx.len()]),
            AudioFormat::ZlibAcc => {
                let n_out = decompress(
                    &mut self.zlib_state,
                    &self.read_buf[..idx.len()],
                    &mut self.decomp_buf,
                )?;
                Ok(&self.decomp_buf[..n_out])
            }
        }
    }

    pub fn get(&mut self, id: &str) -> Result<&[u8], Error> {
        self.get_by_idx(self.index.get_by_id(id)?)
    }
}
