use miniserde::{json, Deserialize};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use crate::{audio::Audio, key::Keys, pages::Pages, Error};

pub struct MonokakidoDict {
    paths: Paths,
    pub pages: Pages,
    pub audio: Option<Audio>,
    pub keys: Keys,
}

#[derive(Deserialize, Debug)]
struct DictJson {
    #[serde(rename = "DSProductContents")]
    contents: Vec<DSProductContents>,
}

#[derive(Deserialize, Debug)]
struct DSProductContents {
    #[serde(rename = "DSContentDirectory")]
    dir: String,
}

pub struct Paths {
    base_path: PathBuf,
    name: String,
    contents_dir: String,
}

impl Paths {
    fn std_list_path() -> PathBuf {
        PathBuf::from(
            "/Library/Application Support/AppStoreContent/jp.monokakido.Dictionaries/Products/",
        )
    }

    fn std_dict_path(name: &str) -> PathBuf {
        let mut path = Paths::std_list_path();
        path.push(format!("jp.monokakido.Dictionaries.{name}"));
        path
    }

    fn json_path(path: &Path, name: &str) -> PathBuf {
        let mut pb = PathBuf::from(path);
        pb.push("Contents");
        pb.push(format!("{name}.json"));
        pb
    }

    pub(crate) fn contents_path(&self) -> PathBuf {
        let mut pb = PathBuf::from(&self.base_path);
        pb.push("Contents");
        pb.push(&self.contents_dir);
        pb
    }

    pub(crate) fn key_path(&self) -> PathBuf {
        let mut pb = PathBuf::from(&self.base_path);
        pb.push("Contents");
        pb.push(&self.contents_dir);
        pb.push("key");
        pb
    }

    pub(crate) fn headword_key_path(&self) -> PathBuf {
        let mut pb = self.key_path();
        pb.push("headword.keystore");
        pb
    }
}

fn parse_dict_name(fname: &OsStr) -> Option<&str> {
    let fname = fname.to_str()?;
    let dict_prefix = "jp.monokakido.Dictionaries.";
    if fname.starts_with(dict_prefix) {
        Some(&fname[dict_prefix.len()..])
    } else {
        None
    }
}

impl MonokakidoDict {
    pub fn list() -> Result<impl Iterator<Item = Result<String, Error>>, Error> {
        let iter = fs::read_dir(&Paths::std_list_path()).map_err(|_| Error::IOError)?;
        Ok(iter.filter_map(|entry| {
            entry
                .map_err(|_| Error::IOError)
                .map(|e| parse_dict_name(&e.file_name()).map(ToOwned::to_owned))
                .transpose()
        }))
    }

    pub fn open(name: &str) -> Result<Self, Error> {
        let std_path = Paths::std_dict_path(name);
        Self::open_with_path_name(&std_path, name)
    }

    pub fn name(&self) -> &str {
        &self.paths.name
    }

    pub fn open_with_path(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path: PathBuf = path.into();
        let dir_name = path.file_name().ok_or(Error::FopenError)?.to_string_lossy();

        let dict_name = dir_name.rsplit_once(".").ok_or(Error::FopenError)?.0;

        Self::open_with_path_name(&path, dict_name)
    }

    fn open_with_path_name(path: impl Into<PathBuf>, name: &str) -> Result<Self, Error> {
        let base_path = path.into();
        let json_path = Paths::json_path(&base_path, name);
        let json = fs::read_to_string(json_path).map_err(|_| Error::NoDictJsonFound)?;
        let mut json: DictJson = json::from_str(&json).map_err(|_| Error::InvalidDictJson)?;
        let contents = json.contents.pop().ok_or(Error::InvalidDictJson)?;
        let paths = Paths {
            base_path,
            name: name.to_owned(),
            contents_dir: contents.dir,
        };
        let pages = Pages::new(&paths)?;
        let audio = Audio::new(&paths)?;
        let keys = Keys::new(&paths)?;

        Ok(MonokakidoDict {
            paths,
            pages,
            audio,
            keys,
        })
    }
}
