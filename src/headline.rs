use std::{
    fs::File,
    io::{Read, Seek},
};

use crate::{
    abi_utils::{TransmuteSafe, LE32, read_vec},
    dict::Paths,
    Error, PageItemId,
};

mod abi {
    use super::*;

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub(super) struct FileHeader {
        magic1: LE32,
        magic2: LE32,
        pub len: LE32,
        pub rec_offset: LE32,
        pub words_offset: LE32,
        rec_bytes: LE32,
        magic4: LE32,
        magic5: LE32,
    }

    impl FileHeader {
        pub(super) fn validate(&self) -> Result<(), Error> {
            if self.magic1.read() == 0
                && self.magic2.read() == 0x2
                && self.rec_bytes.read() == 0x18
                && self.magic4.read() == 0
                && self.magic5.read() == 0
            {
                Ok(())
            } else {
                Err(Error::KeyFileHeaderValidate)
            }
        }
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub(super) struct Offset {
        pub page_id: LE32,
        pub item_id: u8,
        pub item_type: u8,
        magic1: u16,
        pub offset: LE32,
        magic2: LE32,
        magic3: LE32,
        magic4: LE32,
    }

    unsafe impl TransmuteSafe for FileHeader {}
    unsafe impl TransmuteSafe for Offset {}
}
use abi::{FileHeader, Offset};

pub struct Headlines {
    recs: Vec<Offset>,
    words: Vec<u8>,
}

impl Headlines {
    pub fn new(paths: &Paths) -> Result<Headlines, Error> {
        let mut file = File::open(paths.headline_long_path())?;
        let file_size = file.metadata()?.len() as usize;
        let mut hdr = FileHeader::default();
        file.read_exact(hdr.as_bytes_mut())?;
        hdr.validate()?;

        file.seek(std::io::SeekFrom::Start(hdr.words_offset.read() as u64))?;
        let offsets: Option<Vec<Offset>> = read_vec(&mut file, hdr.rec_offset.us(), hdr.words_offset.us())?;
        let Some(recs) = offsets else { return Err(Error::InvalidIndex); };

        let words: Option<Vec<u8>> = read_vec(&mut file, hdr.words_offset.us(), file_size)?;
        let Some(words) = words else { return Err(Error::InvalidIndex); };

        Ok(Headlines {
            recs,
            words,
        })
    }

    pub fn get(&self, id: PageItemId) -> Result<String, Error> {
        let rec = self.recs.binary_search_by(|rec|
            rec.page_id.read().cmp(&id.page).then(rec.item_id.cmp(&id.item))
        ).map_err(|_| Error::InvalidIndex)?;
        todo!();
    }
}
