// Run this with: cargo run --example streams_example

use pipe::pipe;
use std::io;
use std::io::Write;
use std::thread::{sleep, spawn};
use std::time::Duration;
use streampager::{Pager, Result};

fn main() -> Result<()> {
    let (out_read, mut out_write) = pipe();
    let (err_read, mut err_write) = pipe();
    let (prog_read, mut prog_write) = pipe();
    let infinite_output = std::env::args().nth(1) == Some("inf".to_string());

    let out_thread = spawn(move || -> io::Result<()> {
        if infinite_output {
            let mut i = 0;
            loop {
                i += 1;
                out_write.write_all(format!("this is line {}\n", i).as_bytes())?;
            }
        } else {
            for i in 1..=100 {
                out_write.write_all(b"this is line")?;
                sleep(Duration::from_millis(225));
                out_write.write_all(format!(" {}\n", i).as_bytes())?;
                sleep(Duration::from_millis(225));
            }
            out_write.write_all(b"this is the end of output stream\n")?;
            Ok(())
        }
    });

    let err_thread = spawn(move || -> io::Result<()> {
        for i in 1..=10 {
            err_write.write_all(format!("this is error line {}\n", i).as_bytes())?;
            sleep(Duration::from_millis(4000));
        }
        err_write.write_all(b"this is the end of error stream\n")?;
        Ok(())
    });

    let progress_thread = spawn(move || -> io::Result<()> {
        let mut extra: &[u8] = b"";
        for i in 1..=100 {
            prog_write.write_all(format!("progress step {}\n", i).as_bytes())?;
            prog_write.write_all(extra)?;
            prog_write.write_all(b"\x0c")?;
            if i == 40 {
                extra = b"\x1b[32mprogress can have colors and multiple lines\n";
            }
            sleep(Duration::from_millis(120));
        }
        Ok(())
    });

    let mut pager = Pager::new_using_system_terminal()?;
    pager
        .add_output_stream(out_read, "output stream")?
        .add_error_stream(err_read, "error stream")?
        .set_progress_stream(prog_read)
        .set_interface_mode(std::env::var("MODE").unwrap_or_default().as_ref());

    pager.run()?;

    println!("pager has exited");
    let wait_threads = vec![
        spawn(move || {
            println!("out thread joined: {:?}", out_thread.join());
        }),
        spawn(move || {
            println!("err thread joined: {:?}", err_thread.join());
        }),
        spawn(move || {
            println!("progress thread joined: {:?}", progress_thread.join());
        }),
    ];
    for wait_thread in wait_threads {
        wait_thread.join().unwrap();
    }
    Ok(())
}
