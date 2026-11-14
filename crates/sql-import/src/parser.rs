//! Statement-level tolerant parser: tokens to [`Ir`].
//!
//! Every statement is parsed independently. A statement that cannot be understood is reported as
//! a warning and skipped, so one broken or unsupported statement never aborts the import.

use crate::SqlDialect;
use crate::lexer::{Token, TokenKind};
use crate::model::{
    AlterConstraint, Column, Comment, DefaultExpr, EnumType, Fk, FkAction, ImportWarning, Index,
    IndexCol, Ir, Name, Table, TableConstraint, Text,
};
use std::borrow::Cow;

/// Statements that are routine in dumps and carry no schema: skipped without a warning.
const SILENT_STATEMENTS: &[&str] = &[
    "set",
    "select",
    "begin",
    "commit",
    "start",
    "end",
    "rollback",
    "pragma",
    "use",
    "lock",
    "unlock",
    "drop",
    "grant",
    "revoke",
    "analyze",
    "vacuum",
    "delimiter",
    "reset",
    "show",
];

/// `CREATE <kind>` objects that carry no table structure: skipped without a warning.
const SILENT_CREATE: &[&str] = &["schema", "extension", "sequence", "database", "collation"];

struct ParseError {
    line: u32,
    col: u32,
    message: String,
}

type PResult<T> = Result<T, ParseError>;

/// Parses `tokens` into an [`Ir`], collecting warnings for anything it cannot represent.
pub(crate) fn parse<'a>(tokens: &[Token<'a>], dialect: SqlDialect) -> Ir<'a> {
    let mut ir = Ir::default();
    for stmt in statements(tokens) {
        let mut cur = Cursor::new(stmt);
        if let Err(e) = statement(&mut ir, &mut cur, dialect) {
            ir.warn(e.line, e.col, e.message);
        }
    }
    ir
}

fn statements<'t, 'a>(tokens: &'t [Token<'a>]) -> Vec<&'t [Token<'a>]> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, t) in tokens.iter().enumerate() {
        if matches!(t.kind, TokenKind::Punct(';')) {
            out.extend(tokens.get(start..i).filter(|s| !s.is_empty()));
            start = i + 1;
        }
    }
    out.extend(tokens.get(start..).filter(|s| !s.is_empty()));
    out
}

// ---------------------------------------------------------------------------------------------
// Cursor
// ---------------------------------------------------------------------------------------------

struct Cursor<'t, 'a> {
    toks: &'t [Token<'a>],
    i: usize,
}

impl<'t, 'a> Cursor<'t, 'a> {
    fn new(toks: &'t [Token<'a>]) -> Self {
        Self { toks, i: 0 }
    }

    fn peek(&self) -> Option<&'t Token<'a>> {
        self.toks.get(self.i)
    }

    fn peek_at(&self, n: usize) -> Option<&'t Token<'a>> {
        self.toks.get(self.i + n)
    }

    fn bump(&mut self) -> Option<&'t Token<'a>> {
        let t = self.toks.get(self.i)?;
        self.i += 1;
        Some(t)
    }

    fn at_kw(&self, kw: &str) -> bool {
        self.peek().is_some_and(|t| t.is_kw(kw))
    }

    fn at_kw_n(&self, n: usize, kw: &str) -> bool {
        self.peek_at(n).is_some_and(|t| t.is_kw(kw))
    }

    fn eat_kw(&mut self, kw: &str) -> bool {
        let hit = self.at_kw(kw);
        if hit {
            self.i += 1;
        }
        hit
    }

    fn at_punct(&self, c: char) -> bool {
        self.peek()
            .is_some_and(|t| matches!(t.kind, TokenKind::Punct(p) if p == c))
    }

    fn eat_punct(&mut self, c: char) -> bool {
        let hit = self.at_punct(c);
        if hit {
            self.i += 1;
        }
        hit
    }

    /// Position of the next token, or of the last one when the statement ended early.
    fn pos(&self) -> (u32, u32) {
        self.peek()
            .or_else(|| self.toks.last())
            .map_or((1, 1), |t| (t.line, t.col))
    }

    fn fail<T>(&self, message: impl Into<String>) -> PResult<T> {
        let (line, col) = self.pos();
        Err(ParseError {
            line,
            col,
            message: message.into(),
        })
    }

    fn ident(&mut self) -> PResult<Text<'a>> {
        match self.peek().and_then(|t| text_of(t)) {
            Some(text) => {
                self.i += 1;
                Ok(text)
            }
            None => self.fail("expected an identifier"),
        }
    }

    /// `a`, `a.b` or `a.b.c`; the catalog part of a three-part name is discarded.
    fn path(&mut self) -> PResult<Vec<Text<'a>>> {
        let mut parts = vec![self.ident()?];
        while self.at_punct('.') {
            self.i += 1;
            parts.push(self.ident()?);
        }
        Ok(parts)
    }

    fn name(&mut self) -> PResult<Name<'a>> {
        Ok(name_from(self.path()?))
    }

    /// Consumes a parenthesised group starting at the current `(` and returns its inner tokens.
    /// The flag is `false` when the closing parenthesis is missing (truncated input); the inner
    /// tokens then run to the end of the statement.
    fn balanced(&mut self) -> (&'t [Token<'a>], bool) {
        if !self.at_punct('(') {
            return (&[], true);
        }
        let start = self.i + 1;
        let mut depth = 0usize;
        for (j, t) in self.toks.iter().enumerate().skip(self.i) {
            match t.kind {
                TokenKind::Punct('(') => depth += 1,
                TokenKind::Punct(')') => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let inner = self.toks.get(start..j).unwrap_or(&[]);
                        self.i = j + 1;
                        return (inner, true);
                    }
                }
                _ => {}
            }
        }
        let inner = self.toks.get(start..).unwrap_or(&[]);
        self.i = self.toks.len();
        (inner, false)
    }

    fn skip_balanced(&mut self) {
        let _ = self.balanced();
    }

    fn expect_kw(&mut self, kw: &str) -> PResult<()> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            self.fail(format!("expected `{}`", kw.to_ascii_uppercase()))
        }
    }

    fn string(&mut self) -> Option<String> {
        if let Some(Token {
            kind: TokenKind::Str(s),
            ..
        }) = self.peek()
        {
            self.i += 1;
            Some(s.to_string())
        } else {
            None
        }
    }
}

fn text_of<'a>(t: &Token<'a>) -> Option<Text<'a>> {
    match &t.kind {
        TokenKind::Ident(s) => Some(Cow::Borrowed(*s)),
        TokenKind::Quoted(q) => Some(q.clone()),
        _ => None,
    }
}

fn name_from(mut parts: Vec<Text<'_>>) -> Name<'_> {
    let name = parts.pop().unwrap_or(Cow::Borrowed(""));
    let schema = parts.pop();
    Name { schema, name }
}

/// Splits `inner` on commas that are not nested in parentheses; empty items are dropped.
fn split_commas<'t, 'a>(inner: &'t [Token<'a>]) -> Vec<&'t [Token<'a>]> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, t) in inner.iter().enumerate() {
        match t.kind {
            TokenKind::Punct('(') => depth += 1,
            TokenKind::Punct(')') => depth = depth.saturating_sub(1),
            TokenKind::Punct(',') if depth == 0 => {
                out.extend(inner.get(start..i).filter(|s| !s.is_empty()));
                start = i + 1;
            }
            _ => {}
        }
    }
    out.extend(inner.get(start..).filter(|s| !s.is_empty()));
    out
}

// ---------------------------------------------------------------------------------------------
// Rendering tokens back to SQL text
// ---------------------------------------------------------------------------------------------

fn is_wordlike(k: &TokenKind<'_>) -> bool {
    matches!(
        k,
        TokenKind::Ident(_)
            | TokenKind::Quoted(_)
            | TokenKind::Str(_)
            | TokenKind::Number(_)
            | TokenKind::DollarStr(_)
    )
}

fn render(tokens: &[Token<'_>]) -> String {
    let mut out = String::new();
    let mut prev_word = false;
    for t in tokens {
        let word = is_wordlike(&t.kind);
        if word && prev_word {
            out.push(' ');
        }
        match &t.kind {
            TokenKind::Ident(s) | TokenKind::Number(s) => out.push_str(s),
            TokenKind::Quoted(q) => {
                out.push('"');
                out.push_str(&q.replace('"', "\"\""));
                out.push('"');
            }
            TokenKind::Str(s) => {
                out.push('\'');
                out.push_str(&s.replace('\'', "''"));
                out.push('\'');
            }
            TokenKind::DollarStr(s) => {
                out.push_str("$$");
                out.push_str(s);
                out.push_str("$$");
            }
            TokenKind::Punct(c) | TokenKind::Unknown(c) => out.push(*c),
        }
        prev_word = word;
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------------------------

fn statement<'a>(ir: &mut Ir<'a>, cur: &mut Cursor<'_, 'a>, dialect: SqlDialect) -> PResult<()> {
    let Some(first) = cur.peek() else {
        return Ok(());
    };
    if first.is_kw("create") {
        cur.i += 1;
        create(ir, cur, dialect)
    } else if first.is_kw("alter") {
        cur.i += 1;
        alter(ir, cur, dialect)
    } else if first.is_kw("comment") {
        cur.i += 1;
        comment_on(ir, cur)
    } else if SILENT_STATEMENTS.iter().any(|k| first.is_kw(k)) {
        Ok(())
    } else {
        unsupported(ir, cur, 0);
        Ok(())
    }
}

/// Warns about a skipped statement, quoting its first keywords (from `skip` onwards).
fn unsupported(ir: &mut Ir<'_>, cur: &Cursor<'_, '_>, skip: usize) {
    let words: Vec<&str> = cur
        .toks
        .iter()
        .take(skip + 2)
        .filter_map(|t| match &t.kind {
            TokenKind::Ident(s) => Some(*s),
            _ => None,
        })
        .collect();
    let (line, col) = cur.toks.first().map_or((1, 1), |t| (t.line, t.col));
    ir.warn(
        line,
        col,
        format!(
            "skipped unsupported statement: {}",
            words.join(" ").to_uppercase()
        ),
    );
}

fn create<'a>(ir: &mut Ir<'a>, cur: &mut Cursor<'_, 'a>, dialect: SqlDialect) -> PResult<()> {
    let mut unique = false;
    loop {
        if cur.eat_kw("unique") {
            unique = true;
        } else if [
            "or",
            "replace",
            "temporary",
            "temp",
            "unlogged",
            "global",
            "local",
            "fulltext",
            "spatial",
        ]
        .iter()
        .any(|k| cur.eat_kw(k))
        {
        } else {
            break;
        }
    }
    if cur.eat_kw("table") {
        create_table(ir, cur, dialect)
    } else if cur.eat_kw("type") {
        create_type(ir, cur)
    } else if cur.eat_kw("index") {
        create_index(ir, cur, unique)
    } else {
        if !SILENT_CREATE.iter().any(|k| cur.at_kw(k)) {
            unsupported(ir, cur, 0);
        }
        Ok(())
    }
}

fn eat_if_not_exists(cur: &mut Cursor<'_, '_>) {
    if cur.at_kw("if") && cur.at_kw_n(1, "not") && cur.at_kw_n(2, "exists") {
        cur.i += 3;
    }
}

fn create_type<'a>(ir: &mut Ir<'a>, cur: &mut Cursor<'_, 'a>) -> PResult<()> {
    let name = cur.name()?;
    let (line, col) = cur.pos();
    if !(cur.eat_kw("as") && cur.eat_kw("enum")) {
        ir.warn(
            line,
            col,
            format!(
                "skipped unsupported CREATE TYPE `{}` (only ENUM is supported)",
                name.name
            ),
        );
        return Ok(());
    }
    let (inner, _) = cur.balanced();
    let values = split_commas(inner)
        .into_iter()
        .filter_map(|item| match item.first().map(|t| &t.kind) {
            Some(TokenKind::Str(s)) => Some(s.to_string()),
            _ => None,
        })
        .collect();
    ir.enums.push(EnumType { name, values });
    Ok(())
}

fn create_index<'a>(ir: &mut Ir<'a>, cur: &mut Cursor<'_, 'a>, unique: bool) -> PResult<()> {
    let (line, col) = cur.pos();
    cur.eat_kw("concurrently");
    eat_if_not_exists(cur);
    let name = if cur.at_kw("on") {
        None
    } else {
        Some(cur.ident()?)
    };
    cur.expect_kw("on")?;
    cur.eat_kw("only");
    let table = cur.name()?;
    if cur.eat_kw("using") {
        cur.bump();
    }
    if !cur.at_punct('(') {
        return cur.fail("expected `(` before the index column list");
    }
    let (inner, _) = cur.balanced();
    ir.indexes.push(Index {
        name,
        table,
        cols: index_cols(inner),
        unique,
        line,
        col,
    });
    Ok(())
}

fn index_cols<'a>(inner: &[Token<'a>]) -> Vec<IndexCol<'a>> {
    split_commas(inner)
        .into_iter()
        .filter_map(|item| {
            let (first, rest) = item.split_first()?;
            let prefix_len = matches!(
                rest,
                [
                    Token {
                        kind: TokenKind::Punct('('),
                        ..
                    },
                    Token {
                        kind: TokenKind::Number(_),
                        ..
                    },
                    Token {
                        kind: TokenKind::Punct(')'),
                        ..
                    },
                    ..
                ]
            );
            let trailing_words = rest
                .iter()
                .skip(if prefix_len { 3 } else { 0 })
                .all(|t| matches!(t.kind, TokenKind::Ident(_)));
            match text_of(first) {
                Some(col) if trailing_words => Some(IndexCol::Column(col)),
                _ => Some(IndexCol::Expr(render(item))),
            }
        })
        .collect()
}

fn comment_on<'a>(ir: &mut Ir<'a>, cur: &mut Cursor<'_, 'a>) -> PResult<()> {
    if !cur.eat_kw("on") {
        return Ok(());
    }
    let is_column = if cur.eat_kw("table") {
        false
    } else if cur.eat_kw("column") {
        true
    } else {
        return Ok(());
    };
    let mut path = cur.path()?;
    cur.expect_kw("is")?;
    let Some(text) = cur.string() else {
        return Ok(());
    };
    let column = if is_column { path.pop() } else { None };
    ir.comments.push(Comment {
        table: name_from(path),
        column,
        text,
    });
    Ok(())
}

fn alter<'a>(ir: &mut Ir<'a>, cur: &mut Cursor<'_, 'a>, dialect: SqlDialect) -> PResult<()> {
    if !cur.eat_kw("table") {
        return Ok(());
    }
    if cur.at_kw("if") && cur.at_kw_n(1, "exists") {
        cur.i += 2;
    }
    cur.eat_kw("only");
    let table = cur.name()?;
    let rest = cur.toks.get(cur.i..).unwrap_or(&[]);
    for action in split_commas(rest) {
        let mut c = Cursor::new(action);
        if !c.eat_kw("add") {
            continue; // OWNER TO, ALTER COLUMN, ENABLE ROW LEVEL SECURITY, ...
        }
        let (line, col) = c.pos();
        if c.at_kw("column") {
            ir.warn(
                line,
                col,
                format!("ALTER TABLE `{}` ADD COLUMN is not applied", table.name),
            );
            continue;
        }
        match table_item(&mut c, dialect, &table, &mut ir.warnings) {
            Ok(Item::Constraint(constraint)) => ir.alter_constraints.push(AlterConstraint {
                table: table.clone(),
                constraint,
                line,
                col,
            }),
            Ok(Item::Index(index)) => ir.indexes.push(index),
            Ok(Item::Column(_)) => {
                ir.warn(
                    line,
                    col,
                    format!("ALTER TABLE `{}` ADD COLUMN is not applied", table.name),
                );
            }
            Ok(Item::Ignored) => {}
            Err(e) => ir.warn(e.line, e.col, e.message),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// CREATE TABLE
// ---------------------------------------------------------------------------------------------

enum Item<'a> {
    Column(Column<'a>),
    Constraint(TableConstraint<'a>),
    Index(Index<'a>),
    Ignored,
}

fn create_table<'a>(ir: &mut Ir<'a>, cur: &mut Cursor<'_, 'a>, dialect: SqlDialect) -> PResult<()> {
    eat_if_not_exists(cur);
    let (line, col) = cur.pos();
    let name = cur.name()?;
    if !cur.at_punct('(') {
        let (line, col) = cur.pos();
        ir.warn(
            line,
            col,
            format!(
                "skipped unsupported CREATE TABLE form for `{}` (no column list)",
                name.name
            ),
        );
        return Ok(());
    }
    let (open_line, open_col) = cur.pos();
    let (inner, closed) = cur.balanced();
    if !closed {
        ir.warn(
            open_line,
            open_col,
            format!(
                "table `{}` is not terminated; kept the items read so far",
                name.name
            ),
        );
    }
    let mut table = Table {
        name: name.clone(),
        columns: Vec::new(),
        constraints: Vec::new(),
        comment: None,
        line,
        col,
    };
    for item in split_commas(inner) {
        let mut c = Cursor::new(item);
        match table_item(&mut c, dialect, &name, &mut ir.warnings) {
            Ok(Item::Column(col)) => table.columns.push(col),
            Ok(Item::Constraint(k)) => table.constraints.push(k),
            Ok(Item::Index(i)) => ir.indexes.push(i),
            Ok(Item::Ignored) => {}
            Err(e) => ir.warn(e.line, e.col, e.message),
        }
    }
    // MySQL table options: `... ) ENGINE=InnoDB COMMENT='text'`.
    while cur.peek().is_some() {
        if cur.eat_kw("comment") {
            cur.eat_punct('=');
            table.comment = cur.string();
        } else {
            cur.i += 1;
        }
    }
    ir.tables.push(table);
    Ok(())
}

fn table_item<'a>(
    c: &mut Cursor<'_, 'a>,
    dialect: SqlDialect,
    table: &Name<'a>,
    warns: &mut Vec<ImportWarning>,
) -> PResult<Item<'a>> {
    let mysql = dialect == SqlDialect::Mysql;
    let (line, col) = c.pos();
    let constraint_name = if c.eat_kw("constraint")
        && !c.at_kw("primary")
        && !c.at_kw("unique")
        && !c.at_kw("foreign")
        && !c.at_kw("check")
    {
        Some(c.ident()?)
    } else {
        None
    };
    if c.eat_kw("primary") {
        c.eat_kw("key");
        return Ok(Item::Constraint(TableConstraint::PrimaryKey(col_list(c)?)));
    }
    if c.eat_kw("unique") {
        if !c.eat_kw("key") {
            c.eat_kw("index");
        }
        let name = if c.at_punct('(') {
            constraint_name
        } else {
            Some(c.ident()?)
        };
        return Ok(Item::Constraint(TableConstraint::Unique {
            name,
            cols: col_list(c)?,
        }));
    }
    if c.eat_kw("foreign") {
        c.eat_kw("key");
        let name = if c.at_punct('(') {
            constraint_name
        } else {
            Some(c.ident()?)
        };
        let from_cols = col_list(c)?;
        c.expect_kw("references")?;
        let mut fk = references(c, from_cols)?;
        fk.name = name;
        return Ok(Item::Constraint(TableConstraint::ForeignKey(fk)));
    }
    if c.at_kw("check") || c.at_kw("exclude") {
        return Ok(Item::Ignored);
    }
    if c.at_kw("like") {
        return c.fail("LIKE clause in CREATE TABLE is not supported");
    }
    if mysql
        && ["key", "index", "fulltext", "spatial"]
            .iter()
            .any(|k| c.eat_kw(k))
    {
        if !c.eat_kw("key") {
            c.eat_kw("index");
        }
        let name = if c.at_punct('(') {
            None
        } else {
            Some(c.ident()?)
        };
        let (inner, _) = c.balanced();
        return Ok(Item::Index(Index {
            name,
            table: table.clone(),
            cols: index_cols(inner),
            unique: false,
            line,
            col,
        }));
    }
    column(c, dialect, warns).map(Item::Column)
}

fn col_list<'a>(c: &mut Cursor<'_, 'a>) -> PResult<Vec<Text<'a>>> {
    if !c.at_punct('(') {
        return c.fail("expected a parenthesised column list");
    }
    let (inner, _) = c.balanced();
    Ok(split_commas(inner)
        .into_iter()
        .filter_map(|item| item.first().and_then(text_of))
        .collect())
}

fn references<'a>(c: &mut Cursor<'_, 'a>, from_cols: Vec<Text<'a>>) -> PResult<Fk<'a>> {
    let (line, col) = c.pos();
    let to_table = c.name()?;
    let to_cols = if c.at_punct('(') {
        col_list(c)?
    } else {
        Vec::new()
    };
    let mut fk = Fk {
        name: None,
        from_cols,
        to_table,
        to_cols,
        on_delete: None,
        on_update: None,
        line,
        col,
    };
    loop {
        if c.eat_kw("match") {
            c.bump();
        } else if c.at_kw("on") && (c.at_kw_n(1, "delete") || c.at_kw_n(1, "update")) {
            c.i += 1;
            let is_delete = c.eat_kw("delete");
            if !is_delete {
                c.i += 1;
            }
            let action = fk_action(c)?;
            if is_delete {
                fk.on_delete = Some(action);
            } else {
                fk.on_update = Some(action);
            }
        } else if c.at_kw("not") && c.at_kw_n(1, "deferrable") {
            c.i += 2;
        } else if c.eat_kw("deferrable") {
        } else if c.eat_kw("initially") {
            c.bump();
        } else {
            break;
        }
    }
    Ok(fk)
}

fn fk_action(c: &mut Cursor<'_, '_>) -> PResult<FkAction> {
    if c.eat_kw("cascade") {
        Ok(FkAction::Cascade)
    } else if c.eat_kw("restrict") {
        Ok(FkAction::Restrict)
    } else if c.eat_kw("no") && c.eat_kw("action") {
        Ok(FkAction::NoAction)
    } else if c.eat_kw("set") {
        let action = if c.eat_kw("null") {
            FkAction::SetNull
        } else if c.eat_kw("default") {
            FkAction::SetDefault
        } else {
            return c.fail("expected NULL or DEFAULT after SET");
        };
        c.skip_balanced(); // PostgreSQL 15 column list: SET NULL (col)
        Ok(action)
    } else {
        c.fail("unknown referential action")
    }
}

// ---------------------------------------------------------------------------------------------
// Columns
// ---------------------------------------------------------------------------------------------

/// Keywords that end the type of a column and start its constraints.
fn ends_type(c: &Cursor<'_, '_>) -> bool {
    const STOP: &[&str] = &[
        "constraint",
        "not",
        "null",
        "default",
        "primary",
        "unique",
        "references",
        "check",
        "generated",
        "auto_increment",
        "autoincrement",
        "comment",
        "collate",
        "charset",
        "on",
        "unsigned",
        "signed",
        "zerofill",
        "as",
        "key",
        "deferrable",
        "initially",
    ];
    STOP.iter().any(|k| c.at_kw(k)) || (c.at_kw("character") && c.at_kw_n(1, "set"))
}

fn column<'a>(
    c: &mut Cursor<'_, 'a>,
    dialect: SqlDialect,
    warns: &mut Vec<ImportWarning>,
) -> PResult<Column<'a>> {
    let name = c.ident()?;
    let (ty, enum_values) = column_type(c);
    let mut col = Column {
        name,
        ty,
        not_null: false,
        default: None,
        unique: false,
        primary_key: false,
        auto_increment: false,
        references: None,
        enum_values,
        comment: None,
    };
    while c.peek().is_some() {
        column_constraint(c, &mut col, dialect, warns)?;
    }
    if dialect == SqlDialect::Postgres {
        let (base, serial) = match col.ty.to_ascii_lowercase().as_str() {
            "serial" | "serial4" => ("integer", true),
            "bigserial" | "serial8" => ("bigint", true),
            "smallserial" | "serial2" => ("smallint", true),
            _ => ("", false),
        };
        if serial {
            base.clone_into(&mut col.ty);
            col.auto_increment = true;
        }
    }
    Ok(col)
}

fn column_type(c: &mut Cursor<'_, '_>) -> (String, Option<Vec<String>>) {
    let mut ty = String::new();
    let mut enum_values = None;
    while let Some(t) = c.peek() {
        if ends_type(c) {
            break;
        }
        match &t.kind {
            TokenKind::Ident(w) => {
                if !ty.is_empty() && !ty.ends_with('.') {
                    ty.push(' ');
                }
                ty.push_str(w);
                c.i += 1;
            }
            TokenKind::Quoted(q) if ty.is_empty() || ty.ends_with('.') => {
                ty.push_str(q);
                c.i += 1;
            }
            TokenKind::Punct('.') => {
                ty.push('.');
                c.i += 1;
            }
            TokenKind::Punct('(') => {
                let (inner, _) = c.balanced();
                if ty.eq_ignore_ascii_case("enum") {
                    enum_values = Some(
                        split_commas(inner)
                            .into_iter()
                            .filter_map(|i| match i.first().map(|t| &t.kind) {
                                Some(TokenKind::Str(s)) => Some(s.to_string()),
                                _ => None,
                            })
                            .collect(),
                    );
                } else {
                    // Truncated input can leave dangling commas or nothing at all (`varchar(`).
                    let args = render(inner);
                    let args = args.trim_matches(',');
                    if !args.is_empty() {
                        ty.push('(');
                        ty.push_str(args);
                        ty.push(')');
                    }
                }
            }
            TokenKind::Punct('[') => {
                c.i += 1;
                while c.peek().is_some() && !c.eat_punct(']') {
                    c.i += 1;
                }
                ty.push_str("[]");
            }
            _ => break,
        }
    }
    (ty, enum_values)
}

fn column_constraint<'a>(
    c: &mut Cursor<'_, 'a>,
    col: &mut Column<'a>,
    dialect: SqlDialect,
    warns: &mut Vec<ImportWarning>,
) -> PResult<()> {
    let (line, pos_col) = c.pos();
    if c.eat_kw("constraint") {
        let _ = c.ident();
    } else if c.eat_kw("not") {
        if c.eat_kw("null") {
            col.not_null = true;
        } else {
            c.eat_kw("deferrable");
        }
    } else if c.eat_kw("default") {
        col.default = Some(default_expr(c));
    } else if c.eat_kw("primary") {
        c.eat_kw("key");
        col.primary_key = true;
    } else if c.eat_kw("key") {
        col.primary_key = true;
    } else if c.eat_kw("unique") {
        c.eat_kw("key");
        col.unique = true;
    } else if c.eat_kw("references") {
        col.references = Some(references(c, vec![col.name.clone()])?);
    } else if c.eat_kw("check") {
        let (inner, _) = c.balanced();
        if dialect == SqlDialect::Sqlite
            && let Some(values) = in_list_check(inner, &col.name)
        {
            col.enum_values = Some(values);
            warns.push(ImportWarning {
                line,
                column: pos_col,
                message: format!(
                    "enum values of column `{}` were inferred from its CHECK constraint",
                    col.name
                ),
            });
        }
    } else if c.eat_kw("generated") {
        c.eat_kw("always");
        if c.eat_kw("by") {
            c.eat_kw("default");
        }
        generated_as(c, col);
    } else if c.at_kw("as") {
        generated_as(c, col);
    } else if c.eat_kw("auto_increment") || c.eat_kw("autoincrement") {
        col.auto_increment = true;
    } else if c.eat_kw("comment") {
        c.eat_punct('=');
        col.comment = c.string();
    } else if c.eat_kw("collate") || c.eat_kw("charset") {
        c.bump();
    } else if c.eat_kw("character") {
        c.eat_kw("set");
        c.bump();
    } else if c.eat_kw("on") {
        // `ON UPDATE CURRENT_TIMESTAMP(6)` (MySQL) / `ON CONFLICT REPLACE` (SQLite)
        c.bump();
        c.bump();
        c.skip_balanced();
    } else if c.eat_kw("initially") {
        c.bump();
    } else {
        // asc/desc, unsigned, zerofill, deferrable, stored, ... or unknown: skip one token.
        c.bump();
    }
    Ok(())
}

/// Handles the part after `GENERATED ... ` / a bare `AS`: identity column or generated expression.
fn generated_as(c: &mut Cursor<'_, '_>, col: &mut Column<'_>) {
    if !c.eat_kw("as") {
        return;
    }
    if c.eat_kw("identity") {
        col.auto_increment = true;
        c.skip_balanced();
    } else {
        c.skip_balanced();
        if !c.eat_kw("stored") {
            c.eat_kw("virtual");
        }
    }
}

/// Recognises `col IN ('a', 'b', ...)` and returns the literal list.
fn in_list_check(inner: &[Token<'_>], column: &str) -> Option<Vec<String>> {
    let mut c = Cursor::new(inner);
    let name = c.ident().ok()?;
    if !name.eq_ignore_ascii_case(column) || !c.eat_kw("in") {
        return None;
    }
    let (list, _) = c.balanced();
    let items = split_commas(list);
    let values: Vec<String> = items
        .iter()
        .filter_map(|i| match i {
            [
                Token {
                    kind: TokenKind::Str(s),
                    ..
                },
            ] => Some(s.to_string()),
            _ => None,
        })
        .collect();
    (!values.is_empty() && values.len() == items.len()).then_some(values)
}

fn default_expr(c: &mut Cursor<'_, '_>) -> DefaultExpr {
    let start = c.i;
    match c.peek().map(|t| &t.kind) {
        Some(TokenKind::Punct('(')) => c.skip_balanced(),
        Some(TokenKind::Punct('-' | '+')) => {
            c.i += 1;
            c.bump();
        }
        Some(_) => {
            c.i += 1;
            if c.at_punct('(') {
                c.skip_balanced();
            }
        }
        None => {}
    }
    while let Some(t) = c.peek() {
        let continues = match &t.kind {
            TokenKind::Punct('(') => {
                c.skip_balanced();
                continue;
            }
            TokenKind::Punct(':' | '+' | '-' | '*' | '/' | '.')
            | TokenKind::Str(_)
            | TokenKind::Number(_)
            | TokenKind::Quoted(_) => true,
            TokenKind::Ident(_) => !ends_type(c),
            _ => false,
        };
        if !continues {
            break;
        }
        c.i += 1;
    }
    let toks = c.toks.get(start..c.i).unwrap_or(&[]);
    classify_default(toks)
}

fn classify_default(toks: &[Token<'_>]) -> DefaultExpr {
    match toks {
        [
            Token {
                kind: TokenKind::Ident(w),
                ..
            },
        ] if w.eq_ignore_ascii_case("null") => DefaultExpr::Null,
        [
            Token {
                kind: TokenKind::Ident(w),
                ..
            },
        ] if w.eq_ignore_ascii_case("true") => DefaultExpr::Bool(true),
        [
            Token {
                kind: TokenKind::Ident(w),
                ..
            },
        ] if w.eq_ignore_ascii_case("false") => DefaultExpr::Bool(false),
        [
            Token {
                kind: TokenKind::Number(n),
                ..
            },
        ] => DefaultExpr::Number((*n).to_owned()),
        [
            Token {
                kind: TokenKind::Punct(sign @ ('-' | '+')),
                ..
            },
            Token {
                kind: TokenKind::Number(n),
                ..
            },
        ] => DefaultExpr::Number(format!("{sign}{n}")),
        // `'x'::text` and `'x'::character varying`: keep the literal only.
        [
            Token {
                kind: TokenKind::Str(s),
                ..
            },
            rest @ ..,
        ] if rest.is_empty()
            || matches!(rest.first().map(|t| &t.kind), Some(TokenKind::Punct(':'))) =>
        {
            DefaultExpr::Str(s.to_string())
        }
        _ => DefaultExpr::Raw(render(toks)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use proptest::prelude::*;

    const PG: &str = include_str!("../tests/fixtures/postgres.sql");
    const MYSQL: &str = include_str!("../tests/fixtures/mysql.sql");
    const SQLITE: &str = include_str!("../tests/fixtures/sqlite.sql");

    fn ir(sql: &str, dialect: SqlDialect) -> Ir<'_> {
        parse(&lex(sql, dialect), dialect)
    }

    fn table<'a, 'b>(ir: &'b Ir<'a>, name: &str) -> &'b Table<'a> {
        ir.tables
            .iter()
            .find(|t| t.name.name == name)
            .unwrap_or_else(|| panic!("table {name} not found"))
    }

    fn col<'a, 'b>(t: &'b Table<'a>, name: &str) -> &'b Column<'a> {
        t.columns
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("column {name} not found"))
    }

    #[test]
    fn parse_should_match_postgres_fixture_snapshot() {
        insta::assert_debug_snapshot!(ir(PG, SqlDialect::Postgres));
    }

    #[test]
    fn parse_should_match_mysql_fixture_snapshot() {
        insta::assert_debug_snapshot!(ir(MYSQL, SqlDialect::Mysql));
    }

    #[test]
    fn parse_should_match_sqlite_fixture_snapshot() {
        insta::assert_debug_snapshot!(ir(SQLITE, SqlDialect::Sqlite));
    }

    #[test]
    fn parse_should_keep_columns_read_before_a_syntax_error() {
        let sql = "CREATE TABLE t (a int, b varchar(10) REFERENCES, c int);";
        let r = ir(sql, SqlDialect::Postgres);
        let names: Vec<_> = table(&r, "t")
            .columns
            .iter()
            .map(|c| c.name.to_string())
            .collect();
        assert_eq!(names, ["a", "c"]);
    }

    #[test]
    fn parse_should_warn_about_the_item_that_failed() {
        let r = ir(
            "CREATE TABLE t (a int, b int REFERENCES);",
            SqlDialect::Postgres,
        );
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn parse_should_keep_items_of_unterminated_table() {
        let r = ir("CREATE TABLE t (a int, b te", SqlDialect::Postgres);
        assert_eq!(table(&r, "t").columns.len(), 2);
    }

    #[test]
    fn parse_should_warn_and_continue_after_broken_statement() {
        let r = ir(
            "CREATE TABLE ; CREATE TABLE ok (a int);",
            SqlDialect::Postgres,
        );
        assert_eq!((r.warnings.len(), r.tables.len()), (1, 1));
    }

    #[test]
    fn parse_should_warn_for_unsupported_statement_with_its_keywords() {
        let r = ir("CREATE VIEW v AS SELECT 1;", SqlDialect::Postgres);
        assert_eq!(
            r.warnings.first().map(|w| w.message.as_str()),
            Some("skipped unsupported statement: CREATE VIEW")
        );
    }

    #[test]
    fn parse_should_skip_routine_statements_silently() {
        let r = ir(
            "SET x = 1; BEGIN; COMMIT; DROP TABLE a;",
            SqlDialect::Postgres,
        );
        assert_eq!(r.warnings.len(), 0);
    }

    #[test]
    fn parse_should_read_multiword_types() {
        let r = ir(PG, SqlDialect::Postgres);
        let users = table(&r, "users");
        assert_eq!(col(users, "created_at").ty, "timestamp with time zone");
        assert_eq!(col(users, "email").ty, "character varying(255)");
        assert_eq!(col(users, "balance").ty, "numeric(10,2)");
        assert_eq!(col(users, "tags").ty, "text[]");
    }

    #[test]
    fn parse_should_normalise_postgres_serial_to_increment() {
        let r = ir(PG, SqlDialect::Postgres);
        let id = col(table(&r, "items"), "id");
        assert_eq!(
            (id.ty.as_str(), id.auto_increment, id.primary_key),
            ("integer", true, true)
        );
    }

    #[test]
    fn parse_should_read_identity_columns_as_auto_increment() {
        let r = ir(PG, SqlDialect::Postgres);
        assert!(col(table(&r, "orders"), "id").auto_increment);
    }

    #[test]
    fn parse_should_read_inline_foreign_key_with_actions() {
        let r = ir(PG, SqlDialect::Postgres);
        let fk = col(table(&r, "orders"), "user_id").references.clone();
        assert_eq!(fk.map(|f| f.on_delete), Some(Some(FkAction::Cascade)));
    }

    #[test]
    fn parse_should_read_table_level_foreign_key_with_actions() {
        let r = ir(PG, SqlDialect::Postgres);
        let found = table(&r, "items").constraints.iter().any(|k| {
            matches!(k, TableConstraint::ForeignKey(f) if f.on_update == Some(FkAction::Restrict))
        });
        assert!(found);
    }

    #[test]
    fn parse_should_collect_alter_table_primary_keys() {
        let r = ir(PG, SqlDialect::Postgres);
        assert_eq!(r.alter_constraints.len(), 2);
    }

    #[test]
    fn parse_should_read_postgres_enum_values_with_escaped_quote() {
        let r = ir(PG, SqlDialect::Postgres);
        assert_eq!(
            r.enums.first().map(|e| e.values.clone()),
            Some(vec![
                "pending".to_owned(),
                "paid".to_owned(),
                "it's done".to_owned()
            ])
        );
    }

    #[test]
    fn parse_should_strip_casts_from_string_defaults() {
        let r = ir(PG, SqlDialect::Postgres);
        let note = col(table(&r, "orders"), "note");
        assert_eq!(note.default, Some(DefaultExpr::Str("n/a".to_owned())));
    }

    #[test]
    fn parse_should_keep_function_defaults_as_raw_expression() {
        let r = ir(PG, SqlDialect::Postgres);
        let created = col(table(&r, "users"), "created_at");
        assert_eq!(created.default, Some(DefaultExpr::Raw("now()".to_owned())));
    }

    #[test]
    fn parse_should_read_signed_number_default() {
        let r = ir(PG, SqlDialect::Postgres);
        assert_eq!(
            col(table(&r, "items"), "qty").default,
            Some(DefaultExpr::Number("-1".to_owned()))
        );
    }

    #[test]
    fn parse_should_read_postgres_indexes_including_expressions() {
        let r = ir(PG, SqlDialect::Postgres);
        let expr = r
            .indexes
            .iter()
            .any(|i| i.cols.contains(&IndexCol::Expr("lower(name)".to_owned())));
        assert!(expr);
    }

    #[test]
    fn parse_should_read_postgres_comments() {
        let r = ir(PG, SqlDialect::Postgres);
        assert_eq!(r.comments.len(), 2);
    }

    #[test]
    fn parse_should_read_mysql_inline_enum_and_drop_charset_clauses() {
        let r = ir(MYSQL, SqlDialect::Mysql);
        let role = col(table(&r, "users"), "role");
        assert_eq!(
            role.enum_values.as_deref(),
            Some(&["admin".to_owned(), "user".to_owned()][..])
        );
    }

    #[test]
    fn parse_should_read_mysql_auto_increment_and_comments() {
        let r = ir(MYSQL, SqlDialect::Mysql);
        let users = table(&r, "users");
        assert_eq!(
            (col(users, "id").auto_increment, users.comment.as_deref()),
            (true, Some("Users"))
        );
    }

    #[test]
    fn parse_should_read_mysql_key_definitions_as_indexes() {
        let r = ir(MYSQL, SqlDialect::Mysql);
        assert_eq!(r.indexes.len(), 1);
    }

    #[test]
    fn parse_should_read_mysql_foreign_key_set_null() {
        let r = ir(MYSQL, SqlDialect::Mysql);
        let found = table(&r, "posts").constraints.iter().any(|k| {
            matches!(k, TableConstraint::ForeignKey(f) if f.on_delete == Some(FkAction::SetNull))
        });
        assert!(found);
    }

    #[test]
    fn parse_should_infer_enum_from_sqlite_check_in_list() {
        let r = ir(SQLITE, SqlDialect::Sqlite);
        let status = col(table(&r, "users"), "status");
        assert_eq!(status.enum_values.as_ref().map(Vec::len), Some(3));
    }

    #[test]
    fn parse_should_warn_when_enum_is_inferred_from_check() {
        let r = ir(SQLITE, SqlDialect::Sqlite);
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn parse_should_read_untyped_sqlite_column() {
        let r = ir(SQLITE, SqlDialect::Sqlite);
        assert_eq!(col(table(&r, "posts"), "body").ty, "");
    }

    #[test]
    fn parse_should_read_sqlite_autoincrement() {
        let r = ir(SQLITE, SqlDialect::Sqlite);
        assert!(col(table(&r, "users"), "id").auto_increment);
    }

    proptest! {
        #[test]
        fn parse_should_never_panic_on_arbitrary_input(s in "\\PC*") {
            for d in [SqlDialect::Postgres, SqlDialect::Mysql, SqlDialect::Sqlite] {
                let _ = ir(&s, d);
            }
        }

        #[test]
        fn parse_should_never_panic_when_fixtures_are_truncated(cut in 0usize..2000) {
            for (sql, d) in [(PG, SqlDialect::Postgres), (MYSQL, SqlDialect::Mysql), (SQLITE, SqlDialect::Sqlite)] {
                let end = sql.char_indices().map(|(i, _)| i).nth(cut).unwrap_or(sql.len());
                let _ = ir(sql.get(..end).unwrap_or(""), d);
            }
        }
    }
}
