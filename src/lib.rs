use std::fs;

use miniz_oxide::inflate::{core as zlib, TINFLStatus as ZStatus};

mod abi;
mod audio;
mod dict;
mod error;
mod key;
mod pages;

pub use dict::MonokakidoDict;
pub use error::Error;
pub use pages::Pages;
pub use audio::Audio;
pub use key::Keys;

fn decompress(
    zlib_state: &mut zlib::DecompressorOxide,
    in_buf: &[u8],
    out_buf: &mut Vec<u8>,
) -> Result<usize, Error> {
    use zlib::inflate_flags as flg;
    use ZStatus::{Done, HasMoreOutput};

    let flags = flg::TINFL_FLAG_PARSE_ZLIB_HEADER | flg::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;
    let mut n_in_total = 0;
    let mut n_out_total = 0;
    zlib_state.init();
    loop {
        let (status, n_in, n_out) = zlib::decompress(
            zlib_state,
            &in_buf[n_in_total..],
            out_buf,
            n_out_total,
            flags,
        );
        n_out_total += n_out;
        n_in_total += n_in;
        match status {
            HasMoreOutput => {
                out_buf.resize(out_buf.len() * 2 + 1, 0);
                continue;
            }
            Done => break,
            _ => return Err(Error::ZlibError),
        }
    }
    if n_in_total != in_buf.len() {
        return Err(Error::IncorrectStreamLength);
    }
    Ok(n_out_total)
}

#[derive(Debug)]
struct ContentsFile {
    seqnum: u32,
    len: usize,
    offset: usize,
    file: fs::File,
}
