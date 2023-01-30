mod nrsc;
mod rsc;

use std::fs;

pub use nrsc::Nrsc;
pub use rsc::Rsc;

use crate::Error;

use miniz_oxide::inflate::{core as zlib, TINFLStatus as ZStatus};

#[derive(Debug)]
struct ResourceFile {
    seqnum: u32,
    len: usize,
    offset: usize,
    file: fs::File,
}

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
