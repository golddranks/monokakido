use std::{
    fmt::Write as _,
    fs::{create_dir_all, File},
    io::Write,
    path::Path,
};

use monokakido::{Error, MonokakidoDict};

fn explode() -> Result<(), Error> {
    let arg = std::env::args().nth(1).ok_or(Error::InvalidArg)?;

    let mut dict = if Path::new(&arg).exists() {
        MonokakidoDict::open_with_path(Path::new(&arg))
    } else {
        MonokakidoDict::open(&arg)
    }?;
    let pages_dir = "./pages/";
    create_dir_all(pages_dir)?;
    let mut path = String::from(pages_dir);
    for idx in dict.pages.idx_iter()? {
        let (id, page) = dict.pages.get_by_idx(idx)?;
        write!(&mut path, "{id:0>10}.xml")?;
        let mut file = File::create(&path)?;
        path.truncate(pages_dir.len());
        file.write_all(page.as_bytes())?;
    }

    if let Some(audio) = &mut dict.audio {
        let audio_dir = "./audio/";
        create_dir_all(audio_dir)?;
        let mut path = String::from(audio_dir);
        for idx in audio.idx_iter()? {
            let (id, page) = dict.pages.get_by_idx(idx)?;
            write!(&mut path, "{id:0>10}.aac")?;
            let mut file = File::create(&path)?;
            path.truncate(pages_dir.len());
            file.write_all(page.as_bytes())?;
        }
    }
    Ok(())
}

fn main() {
    if let Err(err) = explode() {
        eprintln!("{err:?}");
        return;
    };
}
