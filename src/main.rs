use monokakido::MonokakidoDict;

fn main() {
    /*
       for dict in MonokakidoDict::list().unwrap() {
           dbg!(dict.unwrap());
       }
    */
    let mut dict = MonokakidoDict::open("NHKACCENT2").unwrap();

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
        let (word, pages) = dict.keys.get_index_a(idx).unwrap();
        println!("\n{}", word);
        for id in pages {
            println!("{}", dict.pages.get(id).unwrap());
        }
    }

    println!("Index: prefix order");
    for idx in idx_list {
        let (word, pages) = dict.keys.get_index_b(idx).unwrap();
        println!("\n{}", word);
        for id in pages {
            println!("{}", dict.pages.get(id).unwrap());
        }
    }

    println!("Index: suffix order");
    for idx in idx_list {
        let (word, pages) = dict.keys.get_index_c(idx).unwrap();
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

    //let mut stdout = stdout().lock();
    //stdout.write_all(audio).unwrap();
}
