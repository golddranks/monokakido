use monokakido::{Error, MonokakidoDict};

fn print_help() {
    println!("Monokakido CLI. Supported subcommands:");
    println!("list - lists all dictionaries installed in the standard path");
    println!("list_items {{dict}} {{keyword}} - lists all items");
    println!("list_audio {{dict}} {{keyword}} - lists all audio files");
    println!("help - this help");
}

fn list_items(dict_name: &str, keyword: &str) -> Result<(), Error> {
    let mut dict = MonokakidoDict::open(dict_name)?;
    let (_, items) = dict.keys.search_exact(keyword)?;

    for id in items {
        let item = dict.pages.get_item(id)?;
        println!("{item}");
    }
    Ok(())
}

fn list_pages(dict_name: &str, keyword: &str) -> Result<(), Error> {
    let mut dict = MonokakidoDict::open(dict_name)?;
    let (_, items) = dict.keys.search_exact(keyword)?;

    for id in items {
        let page = dict.pages.get_page(id)?;
        println!("{page}");
    }
    Ok(())
}

fn list_audio(dict_name: &str, keyword: &str) -> Result<(), Error> {
    let mut dict = MonokakidoDict::open(dict_name)?;
    let (_, items) = dict.keys.search_exact(keyword)?;

    for id in items {
        for audio in dict.pages.get_item_audio(id)? {
            if let Some((_, audio)) = audio?.split_once("href=\"") {
                if let Some((id, _)) = audio.split_once('"') {
                    println!("{id}");
                }
            }
        }
    }
    Ok(())
}

fn list_dicts() -> Result<(), Error> {
    for dict in MonokakidoDict::list()? {
        println!("{}", dict?);
    }
    Ok(())
}

fn main() {
    let mut args = std::env::args();
    let res = match args.nth(1).as_deref() {
        Some("list_audio") => {
            if let (Some(dict_name), Some(keyword)) = (args.next(), args.next()) {
                list_audio(&dict_name, &keyword)
            } else {
                Err(Error::InvalidArg)
            }
        }
        Some("list_items") => {
            if let (Some(dict_name), Some(keyword)) = (args.next(), args.next()) {
                list_items(&dict_name, &keyword)
            } else {
                Err(Error::InvalidArg)
            }
        }
        Some("list_pages") => {
            if let (Some(dict_name), Some(keyword)) = (args.next(), args.next()) {
                list_pages(&dict_name, &keyword)
            } else {
                Err(Error::InvalidArg)
            }
        }
        Some("list") => list_dicts(),
        None | Some("help") => {
            print_help();
            Ok(())
        }
        _ => Err(Error::InvalidArg),
    };

    if let Err(e) = res {
        eprintln!("Error: {e:?}");
        std::process::exit(1)
    }
}
