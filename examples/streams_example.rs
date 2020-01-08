// Run this with: cargo run --example streams_example

use pipe::pipe;
use std::io::Write;
use std::thread::{sleep, spawn};
use std::time::Duration;
use streampager::{Pager, Result};

fn main() -> Result<()> {
    let (out_read, mut out_write) = pipe();
    let (err_read, mut err_write) = pipe();
    let (prog_read, mut prog_write) = pipe();

    spawn(move || {
        for i in 1..=100 {
            let _ = out_write.write_all(format!("this is line {}\n", i).as_bytes());
            sleep(Duration::from_millis(450));
        }
        let _ = out_write.write_all(b"this is the end of output stream\n");
    });

    spawn(move || {
        for i in 1..=10 {
            let _ = err_write.write_all(format!("this is error line {}\n", i).as_bytes());
            sleep(Duration::from_millis(4000));
        }
        let _ = err_write.write_all(b"this is the end of error stream\n");
    });

    spawn(move || {
        let mut extra: &[u8] = b"";
        for i in 1..=100 {
            let _ = prog_write.write_all(format!("progress step {}\n", i).as_bytes());
            let _ = prog_write.write_all(extra);
            let _ = prog_write.write_all(b"\x0c");
            if i == 40 {
                extra = b"\x1b[32mprogress can have colors and multiple lines\n";
            }
            sleep(Duration::from_millis(120));
        }
    });

    let mut pager = Pager::new_using_system_terminal()?;
    pager
        .add_output_stream(out_read, "output stream")?
        .add_error_stream(err_read, "error stream")?
        .set_progress_stream(prog_read)
        .set_interface_mode(std::env::var("MODE").unwrap_or_default().as_ref());

    pager.run()?;

    println!("pager has exited");
    Ok(())
}
