use std::{
    io::{stdout, Write},
    ops::Neg,
};

use monokakido::{Error, MonokakidoDict};

fn get_first_audio_id(page: &str) -> Result<&str, Error> {
    if let Some((_, sound_tail)) = page.split_once("<sound>") {
        if let Some((sound, _)) = sound_tail.split_once("</sound>") {
            if let Some((head_id, _)) = sound.split_once(".aac") {
                if let Some((_, id)) = head_id.split_once("href=\"") {
                    return Ok(id);
                }
            }
        }
    }
    Err(Error::NotFound)
}

fn get_first_accent(page: &str) -> Result<i8, Error> {
    if let Some((_, accent_tail)) = page.split_once("<accent_text>") {
        if let Some((mut accent, _)) = accent_tail.split_once("</accent_text>") {
            if let Some((a, _)) = accent.split_once("<sound>") {
                accent = a;
            }
            if let Some(pos) = accent.find("<symbol_backslash>＼</symbol_backslash>") {
                let endpos = pos + "<symbol_backslash>＼</symbol_backslash>".len();
                let before = &accent[..pos];
                let after = &accent[endpos..];
                let is_mora = |&c: &char| {
                    (matches!(c, 'ぁ'..='ん' | 'ァ'..='ン' | 'ー')
                        && !matches!(c, 'ゃ'..='ょ' | 'ャ'..='ョ'))
                };
                return Ok((before.chars().filter(is_mora).count() as i8));
            }
            if let Some(_) = accent.find("<symbol_macron>━</symbol_macron>") {
                return Ok(0);
            }
        }
    }
    Err(Error::NotFound)
}

fn get_accents(page: &str) -> Result<(i8, Option<i8>), Error> {
    if let Some((first, tail)) = page.split_once("</accent>") {
        return Ok((get_first_accent(first)?, get_first_accent(tail).ok()));
    }
    Err(Error::NotFound)
}

fn main() {
    let Some(key) = std::env::args().nth(1) else {
        return;
    };

    for dict in MonokakidoDict::list().unwrap() {
        dbg!(dict.unwrap());
    }

    let mut dict = MonokakidoDict::open("NHKACCENT2").unwrap();
    // let mut accents = vec![];
    let result = dict.keys.search_exact(&key);

    match result {
        Ok((_, pages)) => {
            for id in pages {
                let page = dict.pages.get(id.page).unwrap();
                println!("{page}");
                /*
                if let Ok(accent) = get_accents(page) {
                    accents.push(accent);
                } */
                /*
                let id = get_first_audio_id(page).unwrap();
                let audio = dict.audio.get(id).unwrap();
                let mut stdout = stdout().lock();
                stdout.write_all(audio).unwrap();
                */
            }
        }
        Err(e) => {
            println!("{:?}", e);
            return;
        }
    }
    /*
    print!("{key}\t");
    accents.sort();
    accents.dedup();
    if accents.is_empty() {
        print!("N/A");
    } else {
        for (accent_main, accent_sub) in accents {
            print!("{accent_main}");
            if let Some(accent_sub) = accent_sub {
                if accent_main != accent_sub {
                    print!("/{accent_sub}");
                }
            }
            print!(" ");
        }
    } */


    /*
       let idx_list = [
           0,
           1,
           2,
           3,
           4,
           5,
           6,
           7,
           8,
           9,
           10,
           11,
           12,
           13,
           14,
           15,
           16,
           17,
           18,
           19,
           20,
           46200,
           46201,
           46202,
           46203,
           46204,
           46205,
           46206,
           46207,
           46208,
           46209,
           46210,
           46211,
           70000,
           dict.keys.count() - 1,
       ];

       println!("Index: length order");
       for idx in idx_list {
           let (word, pages) = dict.keys.get_index_len(idx).unwrap();
           println!("\n{}", word);
           for id in pages {
               println!("{}", dict.pages.get(id).unwrap());
           }
       }

       println!("Index: prefix order");
       for idx in idx_list {
           let (word, pages) = dict.keys.get_index_prefix(idx).unwrap();
           println!("\n{}", word);
           for id in pages {
               println!("{}", dict.pages.get(id).unwrap());
           }
       }

       println!("Index: suffix order");
       for idx in idx_list {
           let (word, pages) = dict.keys.get_index_suffix(idx).unwrap();
           println!("\n{}", word);
           for id in pages {
               println!("{}", dict.pages.get(id).unwrap());
           }
       }

       println!("Index: ?");
       for idx in idx_list {
           let (word, pages) = dict.keys.get_index_d(idx).unwrap();
           println!("\n{}", word);
           for id in pages {
               println!("{}", dict.pages.get(id).unwrap());
           }
       }
    */
    let mut audio_rsc = dict.audio.unwrap();
    let audio = audio_rsc.get("jee").unwrap();
    let mut stdout = stdout().lock();
    stdout.write_all(audio).unwrap();
}
