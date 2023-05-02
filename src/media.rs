use std::{fmt::Display, ops::Range, path::PathBuf};

use crate::{
    dict::Paths,
    resource::{Nrsc, Rsc},
    Error,
};

const RSC_NAME: &str = "audio";

pub struct Media {
    path: PathBuf,
    res: Option<MediaResource>,
}

enum MediaResource {
    Rsc(Rsc),
    Nrsc(Nrsc),
}

impl Media {
    pub fn new(paths: &Paths) -> Result<Option<Self>, Error> {
        let mut path = paths.contents_path();
        path.push(RSC_NAME);
        Ok(if path.exists() {
            Some(Media { path, res: None })
        } else {
            None
        })
    }

    pub fn init(&mut self) -> Result<(), Error> {
        if self.res.is_none() {
            self.path.push("index.nidx");
            let nrsc_index_exists = self.path.exists();
            self.path.pop();
            self.res = Some(if nrsc_index_exists {
                MediaResource::Nrsc(Nrsc::new(&self.path)?)
            } else {
                MediaResource::Rsc(Rsc::new(&self.path, RSC_NAME)?)
            });
        }
        Ok(())
    }

    pub fn get(&mut self, id: &str) -> Result<&[u8], Error> {
        self.init()?;
        let Some(res) = self.res.as_mut() else { unreachable!() };
        match res {
            MediaResource::Rsc(rsc) => rsc.get(id.parse::<u32>().map_err(|_| Error::InvalidIndex)?),
            MediaResource::Nrsc(nrsc) => nrsc.get(id),
        }
    }

    pub fn get_by_idx(&mut self, idx: usize) -> Result<(MediaId, &[u8]), Error> {
        self.init()?;
        let Some(res) = self.res.as_mut() else { unreachable!() };
        Ok(match res {
            MediaResource::Rsc(rsc) => {
                let (id, page) = rsc.get_by_idx(idx)?;
                (MediaId::Num(id), page)
            }
            MediaResource::Nrsc(nrsc) => {
                let (id, page) = nrsc.get_by_idx(idx)?;
                (MediaId::Str(id), page)
            }
        })
    }

    pub fn idx_iter(&mut self) -> Result<Range<usize>, Error> {
        self.init()?;
        let Some(res) = self.res.as_ref() else { unreachable!() };
        Ok(0..match res {
            MediaResource::Rsc(rsc) => rsc.len(),
            MediaResource::Nrsc(nrsc) => nrsc.len(),
        })
    }
}

#[derive(Debug)]
pub enum MediaId<'a> {
    Str(&'a str),
    Num(u32),
}

impl Display for MediaId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Str(str) => f.write_str(str),
            Self::Num(num) => write!(f, "{num:0>10}"),
        }
    }
}
