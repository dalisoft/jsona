use crate::App;
use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    files::SimpleFile,
    term::{
        self,
        termcolor::{Ansi, NoColor},
    },
};
use itertools::Itertools;
use jsona::{dom, parser, rowan::TextRange};
use jsona_common::{environment::Environment, schema::jsona_schema::ValidationError};
use std::ops::Range;
use tokio::io::AsyncWriteExt;

impl<E: Environment> App<E> {
    pub(crate) async fn print_parse_errors(
        &self,
        file: &SimpleFile<&str, &str>,
        errors: &[parser::Error],
    ) -> Result<(), anyhow::Error> {
        let mut out_diag = Vec::<u8>::new();

        let config = codespan_reporting::term::Config::default();

        for error in errors.iter().unique_by(|e| e.range) {
            let diag = Diagnostic::error()
                .with_message("invalid JSONA")
                .with_labels(Vec::from([
                    Label::primary((), std_range(error.range)).with_message(&error.message)
                ]));

            if self.colors {
                term::emit(&mut Ansi::new(&mut out_diag), &config, file, &diag)?;
            } else {
                term::emit(&mut NoColor::new(&mut out_diag), &config, file, &diag)?;
            }
        }

        let mut stderr = self.env.stderr();

        stderr.write_all(&out_diag).await?;
        stderr.flush().await?;

        Ok(())
    }

    pub(crate) async fn print_semantic_errors(
        &self,
        file: &SimpleFile<&str, &str>,
        errors: impl Iterator<Item = dom::Error>,
    ) -> Result<(), anyhow::Error> {
        let mut out_diag = Vec::<u8>::new();

        let config = codespan_reporting::term::Config::default();

        for error in errors {
            let diag = match &error {
                dom::Error::ConflictingKeys { key, other } => Diagnostic::error()
                    .with_message(error.to_string())
                    .with_labels(Vec::from([
                        Label::primary((), std_range(key.text_ranges().next().unwrap()))
                            .with_message("duplicate key"),
                        Label::secondary((), std_range(other.text_ranges().next().unwrap()))
                            .with_message("duplicate found here"),
                    ])),
                dom::Error::InvalidEscapeSequence { string } => Diagnostic::error()
                    .with_message(error.to_string())
                    .with_labels(Vec::from([Label::primary(
                        (),
                        std_range(string.text_range()),
                    )
                    .with_message("the string contains invalid escape sequences")])),
                _ => {
                    unreachable!("this is a bug")
                }
            };

            if self.colors {
                term::emit(&mut Ansi::new(&mut out_diag), &config, file, &diag)?;
            } else {
                term::emit(&mut NoColor::new(&mut out_diag), &config, file, &diag)?;
            }
        }
        let mut stderr = self.env.stderr();
        stderr.write_all(&out_diag).await?;
        stderr.flush().await?;
        Ok(())
    }

    pub(crate) async fn print_schema_errors(
        &self,
        file: &SimpleFile<&str, &str>,
        errors: &[ValidationError],
    ) -> Result<(), anyhow::Error> {
        let config = codespan_reporting::term::Config::default();

        let mut out_diag = Vec::<u8>::new();
        for err in errors {
            let msg = err.to_string();
            for text_range in err.node.text_ranges() {
                let diag = Diagnostic::error()
                    .with_message(&msg)
                    .with_labels(Vec::from([
                        Label::primary((), std_range(text_range)).with_message(&msg)
                    ]));

                if self.colors {
                    term::emit(&mut Ansi::new(&mut out_diag), &config, file, &diag)?;
                } else {
                    term::emit(&mut NoColor::new(&mut out_diag), &config, file, &diag)?;
                };
            }
        }
        let mut stderr = self.env.stderr();
        stderr.write_all(&out_diag).await?;
        stderr.flush().await?;

        Ok(())
    }
}

fn std_range(range: TextRange) -> Range<usize> {
    let start: usize = u32::from(range.start()) as _;
    let end: usize = u32::from(range.end()) as _;
    start..end
}