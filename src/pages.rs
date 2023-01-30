use std::{ops::Range, path::PathBuf};

use crate::{dict::Paths, resource::Rsc, Error};

const RSC_NAME: &str = "contents";

pub struct Pages {
    path: PathBuf,
    res: Option<Rsc>,
}

impl Pages {
    pub fn new(paths: &Paths) -> Result<Self, Error> {
        Ok(Pages {
            path: paths.contents_path().join(RSC_NAME),
            res: None,
        })
    }

    pub fn init(&mut self) -> Result<(), Error> {
        if self.res.is_none() {
            self.res = Some(Rsc::new(&self.path, RSC_NAME)?);
        }
        Ok(())
    }

    pub fn get(&mut self, id: u32) -> Result<&str, Error> {
        self.init()?;
        let Some(res) = self.res.as_mut() else { unreachable!() };
        Ok(std::str::from_utf8(res.get(id)?).map_err(|_| Error::Utf8Error)?)
    }

    pub fn get_by_idx(&mut self, idx: usize) -> Result<(u32, &str), Error> {
        self.init()?;
        let Some(res) = self.res.as_mut() else { unreachable!() };
        let (id, page) = res.get_by_idx(idx)?;
        Ok((id, std::str::from_utf8(page).map_err(|_| Error::Utf8Error)?))
    }

    pub fn idx_iter(&mut self) -> Result<Range<usize>, Error> {
        self.init()?;
        let Some(res) = self.res.as_ref() else { unreachable!() };
        Ok(0..res.len())
    }
}
