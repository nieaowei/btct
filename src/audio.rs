use std::{
    io::{BufReader, Cursor},
    thread,
    time::Duration,
};

use include_dir::{include_dir, Dir};
use rodio::{Decoder, OutputStream, Source};

static AUDIO_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/sources");

pub(crate) fn play_file(name: &str, time: u64) {
    let f = AUDIO_DIR.get_file(name).unwrap();

    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    let file = BufReader::new(Cursor::new(f.contents()));
    let source = Decoder::new(file).unwrap();
    stream_handle.play_raw(source.convert_samples());
    thread::sleep(Duration::from_secs(time))
}

pub(crate) fn play_confirmed() {
    play_file("confirmed.mp3", 3);
}

pub(crate) fn play_replaced() {
    play_file("replaced.mp3", 5);
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player() {
        play_file("confirmed.mp3", 3);
    }
}
