/// Input to the foreground program in a session.
#[derive(Clone, Copy, Debug)]
pub enum Input<'a> {
    /// UTF-8 text from normal typing or an input method.
    Text(&'a str),
    /// Pasted text, encoded according to the terminal's current paste mode.
    Paste(&'a str),
    /// Bytes already encoded by the caller, such as a keyboard escape sequence.
    Bytes(&'a [u8]),
}

pub(crate) fn encode(input: Input<'_>, bracketed: bool) -> Vec<u8> {
    match input {
        Input::Text(text) => text.as_bytes().to_vec(),
        Input::Bytes(bytes) => bytes.to_vec(),
        Input::Paste(text) if bracketed => {
            let mut bytes = Vec::with_capacity(text.len() + 12);
            bytes.extend_from_slice(b"\x1b[200~");
            bytes.extend(
                text.bytes()
                    .filter(|byte| !matches!(byte, b'\x1b' | b'\x03')),
            );
            bytes.extend_from_slice(b"\x1b[201~");
            bytes
        }
        Input::Paste(text) => text.replace("\r\n", "\r").replace('\n', "\r").into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Input, encode};

    #[test]
    fn plain_paste_uses_carriage_returns() {
        assert_eq!(
            encode(Input::Paste("one\r\ntwo\nthree"), false),
            b"one\rtwo\rthree"
        );
    }

    #[test]
    fn bracketed_paste_preserves_lines_and_removes_control_bytes() {
        assert_eq!(
            encode(Input::Paste("one\n\x1b[201~\x03two"), true),
            b"\x1b[200~one\n[201~two\x1b[201~"
        );
    }
}
