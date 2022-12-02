use std::{
    fs::File,
    io::{Read, Seek},
    mem::size_of,
    str::from_utf8,
    cmp::Ordering, borrow::Cow,
};

use crate::{
    abi::{TransmuteSafe, LE32},
    dict::Paths,
    Error,
};

mod abi {
    use super::*;

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub(super) struct FileHeader {
        magic1: LE32,
        magic2: LE32,
        pub words_offset: LE32,
        pub idx_offset: LE32,
        magic3: LE32,
        magic4: LE32,
        magic5: LE32,
        magic6: LE32,
    }

    impl FileHeader {
        pub(super) fn validate(&self) -> Result<(), Error> {
            if self.magic1.read() == 0x20000
                && self.magic2.read() == 0
                && self.magic3.read() == 0
                && self.magic4.read() == 0
                && self.magic5.read() == 0
                && self.magic6.read() == 0
                && self.words_offset.us() < self.idx_offset.us()
            {
                Ok(())
            } else {
                Err(Error::Validate)
            }
        }
    }

    unsafe impl TransmuteSafe for FileHeader {}

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub(super) struct IndexHeader {
        magic1: LE32,
        pub index_a_offset: LE32,
        pub index_b_offset: LE32,
        pub index_c_offset: LE32,
        pub index_d_offset: LE32,
    }

    impl IndexHeader {
        pub(super) fn validate(&self, file_end: usize) -> Result<(), Error> {
            if self.magic1.read() == 0x04
                && self.index_a_offset.us() < self.index_b_offset.us()
                && self.index_b_offset.us() < self.index_c_offset.us()
                && self.index_c_offset.us() < self.index_d_offset.us()
                && self.index_d_offset.us() < file_end
            {
                Ok(())
            } else {
                Err(Error::Validate)
            }
        }
    }

    unsafe impl TransmuteSafe for IndexHeader {}
}
use abi::{FileHeader, IndexHeader};

pub struct Keys {
    words: Vec<LE32>,
    index_len: Vec<LE32>,
    index_prefix: Vec<LE32>,
    index_suffix: Vec<LE32>,
    index_d: Vec<LE32>,
}

impl Keys {
    fn read_vec(file: &mut File, start: usize, end: usize) -> Result<Vec<LE32>, Error> {
        let size = (end - start + size_of::<LE32>() - 1) / size_of::<LE32>();
        let mut buf = vec![LE32::default(); size];
        file.read_exact(LE32::slice_as_bytes_mut(&mut buf))?;
        Ok(buf)
    }

    fn check_vec_len(buf: &Vec<LE32>) -> Result<(), Error> {
        if buf.get(0).ok_or(Error::InvalidIndex)?.us() + 1 != buf.len() {
            return Err(Error::InvalidIndex);
        }
        Ok(())
    }

    pub(crate) fn new(paths: &Paths) -> Result<Keys, Error> {
        let mut file = File::open(paths.headword_key_path())?;
        let file_size = file.metadata()?.len() as usize;
        let mut hdr = FileHeader::default();
        file.read_exact(hdr.as_bytes_mut())?;
        hdr.validate()?;

        file.seek(std::io::SeekFrom::Start(hdr.words_offset.read() as u64))?;
        let words = Self::read_vec(&mut file, hdr.words_offset.us(), hdr.idx_offset.us())?;

        if words.get(0).ok_or(Error::InvalidIndex)?.us() + 1 >= words.len() {
            return Err(Error::InvalidIndex);
        }

        let file_end = file_size - hdr.idx_offset.us();
        let mut ihdr = IndexHeader::default();
        file.seek(std::io::SeekFrom::Start(hdr.idx_offset.read() as u64))?;
        file.read_exact(ihdr.as_bytes_mut())?;
        ihdr.validate(file_end)?;

        let index_a = Self::read_vec(
            &mut file,
            ihdr.index_a_offset.us(),
            ihdr.index_b_offset.us(),
        )?;
        Self::check_vec_len(&index_a)?;

        let index_b = Self::read_vec(
            &mut file,
            ihdr.index_b_offset.us(),
            ihdr.index_c_offset.us(),
        )?;
        Self::check_vec_len(&index_b)?;

        let index_c = Self::read_vec(
            &mut file,
            ihdr.index_c_offset.us(),
            ihdr.index_d_offset.us(),
        )?;
        Self::check_vec_len(&index_c)?;

        let index_d = Self::read_vec(&mut file, ihdr.index_d_offset.us(), file_end)?;
        Self::check_vec_len(&index_d)?;

        Ok(Keys {
            words,
            index_len: index_a,
            index_prefix: index_b,
            index_suffix: index_c,
            index_d,
        })
    }

    pub fn count(&self) -> usize {
        // USE INVARIANT A
        self.words[0].us()
    }

    fn get_page_iter(&self, pages_offset: usize) -> Result<PageIter, Error> {
        let pages = &LE32::slice_as_bytes(&self.words)[pages_offset..];
        PageIter::new(pages)
    }

    pub(crate) fn get_word_span(&self, offset: usize) -> Result<(&str, usize), Error> {
        let words_bytes = LE32::slice_as_bytes(&self.words);
        // TODO: add comment. What is this guarding against?
        if words_bytes.len() < offset + 2 * size_of::<LE32>() {
            return Err(Error::InvalidIndex);
        }
        let (pages_offset, word_bytes) = LE32::from(&words_bytes[offset..])?;
        if let Some(word) = word_bytes[1..].split(|b| *b == b'\0').next() {
            Ok((from_utf8(word)?, pages_offset.us()))
        } else {
            Err(Error::InvalidIndex)
        }
    }

    pub(crate) fn cmp_key(&self, target: &str, idx: usize) -> Result<Ordering, Error> {
        let offset = self.index_prefix[idx + 1].us() + size_of::<LE32>() + 1;
        let words_bytes = LE32::slice_as_bytes(&self.words);
        if words_bytes.len() < offset + target.len() + 1 {

            return Err(Error::InvalidIndex); // Maybe just return Ordering::Less instead?
        }
        let found_tail = &words_bytes[offset..];
        let found = &found_tail[..target.len()];
        Ok(match found.cmp(target.as_bytes()) {
            Ordering::Equal => if found_tail[target.len()] == b'\0'
                {
                    Ordering::Equal
                } else {
                    Ordering::Greater
                },
            ord => ord,
        })
    }


    fn get_inner(&self, index: &[LE32], idx: usize) -> Result<(&str, PageIter<'_>), Error> {
        if idx >= self.count() {
            return Err(Error::NotFound);
        }
        let word_offset = index[idx + 1].us();
        let (word, pages_offset) = self.get_word_span(word_offset)?;
        let pages = self.get_page_iter(pages_offset)?;
        Ok((word, pages))
    }

    pub fn get_index_len(&self, idx: usize) -> Result<(&str, PageIter<'_>), Error> {
        self.get_inner(&self.index_len, idx)
    }

    pub fn get_index_prefix(&self, idx: usize) -> Result<(&str, PageIter<'_>), Error> {
        self.get_inner(&self.index_prefix, idx)
    }

    pub fn get_index_suffix(&self, idx: usize) -> Result<(&str, PageIter<'_>), Error> {
        self.get_inner(&self.index_suffix, idx)
    }

    pub fn get_index_d(&self, idx: usize) -> Result<(&str, PageIter<'_>), Error> {
        self.get_inner(&self.index_d, idx)
    }

    pub fn search_exact(&self, target_key: &str) -> Result<(usize, PageIter<'_>), Error> {
        let target_key = &to_katakana(target_key);
        let mut high = self.count();
        let mut low = 0;

        // TODO: Revise corner cases and add tests for this binary search
        while low <= high {
            let mid = low + (high - low) / 2;

            let cmp = self.cmp_key(target_key, mid)?;

            match cmp {
                Ordering::Less => low = mid + 1,
                Ordering::Greater => high = mid - 1,
                Ordering::Equal => return Ok((mid, self.get_index_prefix(mid)?.1)),
            }
        }

        return Err(Error::NotFound);
    }
}

fn to_katakana(input: &str) -> Cow<str> {
    let diff = 'ア' as u32 - 'あ' as u32;
    if let Some(pos) = input.find(|c| matches!(c, 'ぁ'..='ん')) {
        let mut output = input[..pos].to_owned();
        for c in input[pos..].chars() {
            if matches!(c, 'ぁ'..='ん') {
                output.push(char::from_u32(c as u32 + diff).unwrap());
            } else {
                output.push(c);
            }
        }
        return Cow::Owned(output);
    } else {
        return Cow::Borrowed(input);
    }
}

#[test]
fn test_to_katakana() {
    assert_eq!(*to_katakana(""), *"");
    assert_eq!(*to_katakana("あ"), *"ア");
    assert_eq!(*to_katakana("ぁ"), *"ァ");
    assert_eq!(*to_katakana("ん"), *"ン");
    assert_eq!(*to_katakana("っ"), *"ッ");
    assert_eq!(*to_katakana("ア"), *"ア");
    assert_eq!(*to_katakana("ァ"), *"ァ");
    assert_eq!(*to_katakana("ン"), *"ン");
    assert_eq!(*to_katakana("ッ"), *"ッ");
    assert_eq!(*to_katakana("aアa"), *"aアa");
    assert_eq!(*to_katakana("aァa"), *"aァa");
    assert_eq!(*to_katakana("aンa"), *"aンa");
    assert_eq!(*to_katakana("aッa"), *"aッa");
}

#[derive(Debug, Clone)]
pub struct PageIter<'a> {
    count: u16,
    span: &'a [u8],
}

impl<'a> PageIter<'a> {
    fn new(pages: &'a [u8]) -> Result<Self, Error> {
        let (count, pages) = pages.split_at(2);
        let count = u16::from_le_bytes(count.try_into().unwrap());

        // CHECK INVARIANT B: loop through `count` times and check that the shape is of expected
        let mut tail = pages;
        for _ in 0..count {
            match tail {
                &[1, _, ref t @ ..] => tail = t,
                &[2, _, _, ref t @ ..] => tail = t,
                &[4, _, _, _, ref t @ ..] => tail = t,
                e => {
                    dbg!("hmm", &e[..100]);
                    return Err(Error::InvalidIndex);
                },
            }
        }
        let span_len = pages.len() - tail.len();
        Ok(PageIter {
            span: &pages[..span_len],
            count,
        })
    }
}

impl<'a> Iterator for PageIter<'a> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        // USE INVARIANT B: `self.span` is checked to conform to this shape,
        // so unreachable is never reached. `self.count` is also checked to correspond,
        // so overflow never happens.
        let (id, tail) = match self.span {
            &[1, hi, ref tail @ ..] => (u32::from_be_bytes([0, 0, 0, hi]), tail),
            &[2, hi, lo, ref tail @ ..] => (u32::from_be_bytes([0, 0, hi, lo]), tail),
            &[4, hi, mid, lo, ref tail @ ..] => (u32::from_be_bytes([0, hi, mid, lo]), tail),
            &[] => return None,
            _ => unreachable!(),
        };
        self.count -= 1;
        self.span = tail;
        Some(id)
    }
}
