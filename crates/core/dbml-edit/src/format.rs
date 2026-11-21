//! Formatador DBML conservador: só mexe em espaço em branco (indentação, espaços
//! no fim da linha, linhas em branco repetidas). Conteúdo e comentários ficam intactos.

/// Conta `{` e `}` fora de strings e comentários de linha.
fn braces(line: &str) -> (i32, i32) {
    let (mut open, mut close) = (0, 0);
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' | '`' => quote = Some(c),
                '/' if chars.peek() == Some(&'/') => break,
                '{' => open += 1,
                '}' => close += 1,
                _ => {}
            },
        }
    }
    (open, close)
}

/// Formats a DBML document with consistent indentation while preserving comments.
#[must_use]
pub fn format(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth: i32 = 0;
    let mut blank_run = 0;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            blank_run += 1;
            // no máximo uma linha em branco seguida, e nenhuma logo após `{`
            if blank_run == 1 && !out.is_empty() && !out.trim_end().ends_with('{') {
                out.push('\n');
            }
            continue;
        }
        blank_run = 0;
        let (open, close) = braces(line);
        let leading_close = i32::from(line.starts_with('}'));
        let indent = (depth - leading_close).max(0) as usize;
        // remove linha em branco antes de `}`
        if leading_close == 1 && out.ends_with("\n\n") {
            out.pop();
        }
        out.push_str(&"  ".repeat(indent));
        out.push_str(line);
        out.push('\n');
        depth = (depth + open - close).max(0);
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

/// Troca a linha `line` (0-based) com a de cima (`up`) ou a de baixo. Retorna o
/// intervalo de bytes que cobre as duas linhas e o texto que o substitui.
#[must_use]
pub fn swap_lines(text: &str, line: usize, up: bool) -> Option<(std::ops::Range<usize>, String)> {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let n = starts.len();
    let first = if up { line.checked_sub(1)? } else { line };
    if first + 1 >= n {
        return None;
    }
    let end_of = |l: usize| {
        if l + 1 < n {
            starts[l + 1] - 1
        } else {
            text.len()
        }
    };
    let (a, b) = (
        &text[starts[first]..end_of(first)],
        &text[starts[first + 1]..end_of(first + 1)],
    );
    Some((starts[first]..end_of(first + 1), format!("{b}\n{a}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reindents_and_keeps_comments() {
        let src = "// topo\nTable users {\nid int [pk]   \n      // comentário\n  note text [note: '{ não conta }']\n\n\n}\n\n\n\nRef: a.b > c.d\n";
        insta::assert_snapshot!(format(src), @r"
        // topo
        Table users {
          id int [pk]
          // comentário
          note text [note: '{ não conta }']
        }

        Ref: a.b > c.d
        ");
    }

    #[test]
    fn swap_lines_up_and_down() {
        let t = "a\nb\nc";
        let (r, s) = swap_lines(t, 1, true).unwrap();
        assert_eq!(format!("{}{}{}", &t[..r.start], s, &t[r.end..]), "b\na\nc");
        let (r, s) = swap_lines(t, 1, false).unwrap();
        assert_eq!(format!("{}{}{}", &t[..r.start], s, &t[r.end..]), "a\nc\nb");
        assert!(swap_lines(t, 0, true).is_none());
        assert!(swap_lines(t, 2, false).is_none());
    }

    #[test]
    fn idempotent_on_sample() {
        let once = format("Table t {\n    indexes {\n  id\n }\n}\n");
        assert_eq!(format(&once), once);
        assert_eq!(once, "Table t {\n  indexes {\n    id\n  }\n}\n");
    }
}
