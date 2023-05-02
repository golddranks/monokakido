use std::{
    fmt::Write as _,
    fs::{create_dir_all, File},
    io::Write,
};

use monokakido::{Error, KeyIndex, MonokakidoDict, PageItemId};

fn out_dir(dict: &MonokakidoDict) -> String {
    dict.name().to_owned() + "_out/"
}

fn write_index(dict: &MonokakidoDict, index: &KeyIndex, tsv_fname: &str) -> Result<(), Error> {
    let mut index_tsv = File::create(out_dir(dict) + tsv_fname)?;
    for i in 0..index.len() {
        let (id, pages) = dict.keys.get_idx(index, i)?;
        index_tsv.write_all(id.as_bytes())?;
        for PageItemId { page, item } in pages {
            write!(&mut index_tsv, "\t{page:0>10}")?;
            if item > 0 {
                write!(&mut index_tsv, "-{item:0>3}")?;
            }
        }
        index_tsv.write_all(b"\n")?;
    }
    Ok(())
}

fn explode() -> Result<(), Error> {
    let arg = std::env::args().nth(1).ok_or(Error::InvalidArg)?;

    let mut dict = MonokakidoDict::open(&arg)?;

    let pages_dir = out_dir(&dict) + "pages/";
    let audio_dir = out_dir(&dict) + "audio/";
    let graphics_dir = out_dir(&dict) + "graphics/";

    create_dir_all(&pages_dir)?;
    let mut path = String::from(&pages_dir);
    for idx in dict.pages.idx_iter()? {
        let (id, page) = dict.pages.page_by_idx(idx)?;
        write!(&mut path, "{id:0>10}.xml")?;
        let mut file = File::create(&path)?;
        path.truncate(pages_dir.len());
        file.write_all(page.as_bytes())?;
    }

    if let Some(audio) = &mut dict.audio {
        create_dir_all(&audio_dir)?;
        let mut path = String::from(&audio_dir);
        for idx in audio.idx_iter()? {
            let (id, audio) = audio.get_by_idx(idx)?;
            write!(&mut path, "{id}.aac")?;
            let mut file = File::create(&path)?;
            path.truncate(audio_dir.len());
            file.write_all(audio)?;
        }
    }

    if let Some(graphics) = &mut dict.graphics {
        create_dir_all(&graphics_dir)?;
        let mut path = String::from(&graphics_dir);
        for idx in graphics.idx_iter()? {
            let (id, graphics) = graphics.get_by_idx(idx)?;
            write!(&mut path, "{id}")?;
            let mut file = File::create(&path)?;
            path.truncate(graphics_dir.len());
            file.write_all(graphics)?;
        }
    }

    write_index(&dict, &dict.keys.index_len, "index_len.tsv")?;
    write_index(&dict, &dict.keys.index_prefix, "index_prefix.tsv")?;
    write_index(&dict, &dict.keys.index_suffix, "index_suffix.tsv")?;
    write_index(&dict, &dict.keys.index_d, "index_d.tsv")?;
    Ok(())
}

fn main() {
    if let Err(err) = explode() {
        eprintln!("{err:?}");
    };
}
