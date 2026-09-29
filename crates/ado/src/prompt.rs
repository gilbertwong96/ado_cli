//! The interactive-confirmation seam (spec §4.1, D30–D32): a command asks a
//! yes/no question before irreversible work, the binary reads the answer from
//! stdin, and tests inject a scripted answer.
//!
//! The question goes to **stderr** on every path (D31 — stdout carries exactly
//! one document under `--json`), an unanswered question is never a yes (D30 —
//! the frozen CLI's EOF exits 0 silently), and a refusal is a failure: the
//! command returns [`AdoError::cancelled`](ado_core::error::AdoError::cancelled),
//! which exits 1 (D32). The question and the refusal wording belong to the
//! command: the three prompting commands ask three different questions.

use std::io::{self, BufRead, BufReader, Write};
#[cfg(test)]
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

/// A yes/no question asked before an irreversible command runs.
pub trait Confirm {
    /// Asks `question` and reports whether the answer was yes. Implementations
    /// write the question to stderr and never touch stdout.
    fn confirm(&mut self, question: &str) -> bool;
}

/// The binary's [`Confirm`]: the question on stderr, the answer read from
/// stdin. Generic over its streams so tests drive it with in-memory ones and no
/// test ever needs a terminal.
pub struct StdinConfirm<R = BufReader<io::Stdin>, W = io::Stderr> {
    input: R,
    output: W,
}

impl StdinConfirm<BufReader<io::Stdin>, io::Stderr> {
    pub fn new() -> StdinConfirm<BufReader<io::Stdin>, io::Stderr> {
        StdinConfirm::over(BufReader::new(io::stdin()), io::stderr())
    }
}

impl Default for StdinConfirm<BufReader<io::Stdin>, io::Stderr> {
    fn default() -> StdinConfirm<BufReader<io::Stdin>, io::Stderr> {
        StdinConfirm::new()
    }
}

impl<R: BufRead, W: Write> StdinConfirm<R, W> {
    pub fn over(input: R, output: W) -> StdinConfirm<R, W> {
        StdinConfirm { input, output }
    }
}

impl<R: BufRead, W: Write> Confirm for StdinConfirm<R, W> {
    fn confirm(&mut self, question: &str) -> bool {
        // A failed question write must not turn into a silent no: the caller's
        // refusal path (exit 1, nothing done) is the safe outcome either way,
        // and there is no stream left to report the write failure on.
        let _ = write!(self.output, "{question}");
        let _ = self.output.flush();

        let mut answer = String::new();

        match self.input.read_line(&mut answer) {
            // EOF is an unanswered question, and an unanswered question is not a
            // yes (D30): the frozen CLI exits 0 there as if the work happened.
            Ok(0) | Err(_) => false,
            // The frozen helper's test: trimmed and downcased, exactly "y".
            Ok(_) => answer.trim().eq_ignore_ascii_case("y"),
        }
    }
}

/// The answers a test scripts, and the questions the command asked. A clone
/// shares both queues with the context that owns one, so a test keeps a handle
/// for [`ScriptedConfirm::asked`] after injecting the other.
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct ScriptedConfirm {
    answers: Rc<RefCell<VecDeque<bool>>>,
    asked: Rc<RefCell<Vec<String>>>,
}

#[cfg(test)]
impl ScriptedConfirm {
    pub(crate) fn new(answers: impl IntoIterator<Item = bool>) -> ScriptedConfirm {
        ScriptedConfirm {
            answers: Rc::new(RefCell::new(answers.into_iter().collect())),
            asked: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// The questions the command asked, in order.
    pub(crate) fn asked(&self) -> Vec<String> {
        self.asked.borrow().clone()
    }
}

#[cfg(test)]
impl Confirm for ScriptedConfirm {
    fn confirm(&mut self, question: &str) -> bool {
        self.asked.borrow_mut().push(question.to_owned());

        // A question no test answered is a refusal: a test must never hang on a
        // terminal or quietly confirm work it did not script.
        self.answers.borrow_mut().pop_front().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    /// The question and the bytes the implementation wrote, for a scripted answer.
    fn answering(input: &str) -> (bool, String) {
        let mut output = Vec::new();
        let mut confirm = StdinConfirm::over(Cursor::new(input.as_bytes().to_vec()), &mut output);
        let confirmed = confirm.confirm(QUESTION);
        let written = String::from_utf8(output).expect("the question is UTF-8");

        (confirmed, written)
    }

    const QUESTION: &str = "Delete project 'Alpha'? This cannot be undone. [y/N] ";

    #[test]
    fn y_is_yes_and_the_question_goes_to_the_writer() {
        let (confirmed, written) = answering("y\n");

        assert!(confirmed, "y is yes");
        assert_eq!(written, QUESTION, "the question is written verbatim");
    }

    #[test]
    fn y_is_yes_whatever_its_case_or_surrounding_space() {
        for answer in ["Y\n", " y \n", "y\r\n"] {
            let (confirmed, _) = answering(answer);

            assert!(
                confirmed,
                "{answer:?} is a yes: the helper trims and downcases"
            );
        }
    }

    #[test]
    fn any_other_answer_is_no() {
        for answer in ["n\n", "no\n", "yes\n", "\n", "x\n"] {
            let (confirmed, _) = answering(answer);

            assert!(!confirmed, "{answer:?} is not the helper's y");
        }
    }

    #[test]
    fn eof_is_a_refusal() {
        let (confirmed, written) = answering("");

        assert!(!confirmed, "an unanswered question is not a yes (D30)");
        assert_eq!(written, QUESTION, "the question is still asked");
    }

    #[test]
    fn the_scripted_confirm_answers_in_order_and_records_the_questions() {
        let mut confirm = ScriptedConfirm::new([true, false]);

        assert!(confirm.confirm(QUESTION));
        assert!(!confirm.confirm("Close thread 1? [y/N] "));
        assert_eq!(
            confirm.asked(),
            vec![QUESTION.to_owned(), "Close thread 1? [y/N] ".to_owned()],
            "a test can see which questions its command asked"
        );
    }

    #[test]
    fn an_unscripted_question_is_a_refusal() {
        let mut confirm = ScriptedConfirm::new([]);

        assert!(
            !confirm.confirm(QUESTION),
            "a test must never hang on a terminal or silently confirm"
        );
    }
}
