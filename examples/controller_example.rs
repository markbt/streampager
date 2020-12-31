use std::time::Duration;

use anyhow::Result;

use streampager::controlled_file::{Change, Controller};
use streampager::Pager;

fn start_thread(controller: Controller) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        controller
            .apply_changes(vec![
                Change::InsertLine {
                    before_index: 1,
                    content: b"\x1B[1m======\x1B[0m".to_vec(),
                },
                Change::ReplaceLine {
                    index: 0,
                    content: b"\x1B[1;38;5;205mHello!\x1B[0m".to_vec(),
                },
                Change::AppendLines {
                    contents: vec![b"".to_vec(), b"Some new data has arrived!".to_vec()],
                },
            ])
            .unwrap();
    });
}

fn main() -> Result<()> {
    let controller = Controller::new();

    controller.apply_changes(vec![Change::AppendLines {
        contents: vec![
            b"Hello!".to_vec(),
            b"".to_vec(),
            b"This is an example controlled file.".to_vec(),
        ],
    }])?;

    start_thread(controller.clone());

    let mut pager = Pager::new_using_system_terminal()?;

    pager.add_controlled_file(&controller)?;
    pager.run()?;

    Ok(())
}
