use crate::world::{BONUS_PUZZLES, validate_bonus_puzzle};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EditorInput {
    pub submit: bool,
    pub close: bool,
    pub tab: bool,
    pub enter: bool,
    pub backspace: bool,
    pub characters: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditorEvent {
    None,
    Closed,
    Submitted { accepted: bool },
}

#[derive(Default)]
pub(crate) struct EditorState {
    open: bool,
    terminal: usize,
    text: String,
    status: String,
    status_is_error: bool,
    accepted: bool,
}

impl EditorState {
    pub(crate) fn open(&mut self, terminal: usize) {
        self.open = true;
        self.terminal = terminal;
        self.text = BONUS_PUZZLES[terminal].starter.to_owned();
        self.status = "Edit the Rust code - Ctrl+Enter or F5 to check, Esc to close.".to_owned();
        self.status_is_error = false;
        self.accepted = false;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn terminal(&self) -> usize {
        self.terminal
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn status(&self) -> &str {
        &self.status
    }

    pub(crate) fn status_is_error(&self) -> bool {
        self.status_is_error
    }

    pub(crate) fn update(&mut self, input: &EditorInput) -> EditorEvent {
        if input.close {
            self.close();
            return EditorEvent::Closed;
        }
        if input.submit {
            if self.accepted {
                return EditorEvent::None;
            }
            let accepted = self.submit();
            if accepted {
                self.accepted = true;
            }
            return EditorEvent::Submitted { accepted };
        }
        if input.tab {
            self.text.push_str("    ");
        }
        if input.enter {
            self.text.push('\n');
        }
        if input.backspace {
            self.text.pop();
        }
        for character in input.characters.chars() {
            if character == '\n' || character == '\r' || character == '\t' {
                continue;
            }
            if character.is_control() {
                continue;
            }
            self.text.push(character);
        }
        EditorEvent::None
    }

    fn close(&mut self) {
        self.open = false;
        self.status.clear();
    }

    #[cfg(test)]
    pub(crate) fn set_text_for_test(&mut self, text: &str) {
        self.text = text.to_owned();
    }

    fn submit(&mut self) -> bool {
        let solved = validate_bonus_puzzle(self.terminal, &self.text);
        if solved {
            self.status = BONUS_PUZZLES[self.terminal].success.to_owned();
            self.status_is_error = false;
        } else {
            self.status = format!("CHECK FAILED - {}", BONUS_PUZZLES[self.terminal].hint);
            self.status_is_error = true;
        }
        solved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_open_close_and_submit_transitions() {
        let mut editor = EditorState::default();
        editor.open(0);
        assert!(editor.is_open());
        assert_eq!(editor.text(), BONUS_PUZZLES[0].starter);

        assert_eq!(
            editor.update(&EditorInput {
                close: true,
                submit: true,
                ..EditorInput::default()
            }),
            EditorEvent::Closed
        );
        assert!(!editor.is_open());
        assert!(editor.status().is_empty());

        editor.open(0);
        editor.text = "fn main() {
            let timeline = String::from(\"unrun\");
            let past = timeline.clone();
            println!(\"{}\", timeline);
        }"
        .to_owned();
        assert_eq!(
            editor.update(&EditorInput {
                submit: true,
                ..EditorInput::default()
            }),
            EditorEvent::Submitted { accepted: true }
        );
        assert!(editor.is_open());
        assert!(!editor.status_is_error());
        assert_eq!(editor.status(), BONUS_PUZZLES[0].success);
    }

    #[test]
    fn submit_after_accept_is_ignored_until_reopened() {
        let mut editor = EditorState::default();
        editor.open(0);
        editor.text = "fn main() {
            let timeline = String::from(\"unrun\");
            let past = timeline.clone();
            println!(\"{}\", timeline);
        }"
        .to_owned();
        let submit = EditorInput {
            submit: true,
            ..EditorInput::default()
        };

        assert_eq!(
            editor.update(&submit),
            EditorEvent::Submitted { accepted: true }
        );

        // 受理後の再 submit は無視され、成功イベントも状態も変わらない
        assert_eq!(editor.update(&submit), EditorEvent::None);
        assert!(editor.is_open());
        assert!(!editor.status_is_error());
        assert_eq!(editor.status(), BONUS_PUZZLES[0].success);

        // 開き直せば再度 submit できる(失敗時の編集・再 submit 経路は維持)
        editor.open(0);
        assert_eq!(
            editor.update(&submit),
            EditorEvent::Submitted { accepted: false }
        );
        assert!(editor.status_is_error());
    }
}
