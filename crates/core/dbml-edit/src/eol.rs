//! Detecção e preservação do terminador de linha do documento.
//!
//! As edições estruturais trabalham internamente com `\n`. Para que um documento CRLF não
//! termine com terminadores mistos, as funções públicas normalizam a entrada para LF, editam
//! e devolvem o resultado no terminador predominante do original.

/// Predominant line-ending style detected in a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// Unix line feed (`\n`).
    Lf,
    /// Windows carriage-return + line feed (`\r\n`).
    Crlf,
}

impl LineEnding {
    /// Detects the predominant line ending of `text` (LF on ties and on text without breaks).
    #[must_use]
    pub fn detect(text: &str) -> Self {
        let bytes = text.as_bytes();
        let (mut crlf, mut lf) = (0usize, 0usize);
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' {
                if i > 0 && bytes[i - 1] == b'\r' {
                    crlf += 1;
                } else {
                    lf += 1;
                }
            }
        }
        if crlf > lf { Self::Crlf } else { Self::Lf }
    }

    /// Returns the line-ending string slice (`"\n"` or `"\r\n"`).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
        }
    }

    /// Rewrites every line break of `text` using this ending.
    #[must_use]
    pub fn apply(self, text: &str) -> String {
        match self {
            Self::Lf => text.to_string(),
            Self::Crlf => text.replace("\r\n", "\n").replace('\n', "\r\n"),
        }
    }
}

/// Resultado de edição que sabe voltar ao terminador de linha do documento original.
pub(crate) trait Restore {
    fn restore(self, eol: LineEnding) -> Self;
}

impl Restore for String {
    fn restore(self, eol: LineEnding) -> Self {
        eol.apply(&self)
    }
}

impl Restore for Option<String> {
    fn restore(self, eol: LineEnding) -> Self {
        self.map(|s| eol.apply(&s))
    }
}

/// Executa `f` sobre o texto normalizado para LF e devolve o resultado no terminador original.
pub(crate) fn preserving<R: Restore>(text: &str, f: impl FnOnce(&str) -> R) -> R {
    let eol = LineEnding::detect(text);
    match eol {
        LineEnding::Lf => f(text),
        LineEnding::Crlf => f(&text.replace("\r\n", "\n")).restore(eol),
    }
}

/// Declara a variante pública, preservadora de terminador, de uma edição de texto.
macro_rules! eol_fn {
    ($(#[$meta:meta])* $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $ret:ty => $inner:path) => {
        $(#[$meta])*
        #[must_use]
        pub fn $name(text: &str, $($arg: $ty),*) -> $ret {
            $crate::eol::preserving(text, |text| $inner(text, $($arg),*))
        }
    };
}
pub(crate) use eol_fn;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_predominant_ending() {
        assert_eq!(LineEnding::detect(""), LineEnding::Lf);
        assert_eq!(LineEnding::detect("a\nb\n"), LineEnding::Lf);
        assert_eq!(LineEnding::detect("a\r\nb\r\n"), LineEnding::Crlf);
        assert_eq!(LineEnding::detect("a\r\nb\r\nc\n"), LineEnding::Crlf);
        assert_eq!(LineEnding::detect("a\r\nb\nc\n"), LineEnding::Lf);
    }

    #[test]
    fn apply_is_idempotent() {
        let once = LineEnding::Crlf.apply("a\nb\r\nc");
        assert_eq!(once, "a\r\nb\r\nc");
        assert_eq!(LineEnding::Crlf.apply(&once), once);
    }
}
