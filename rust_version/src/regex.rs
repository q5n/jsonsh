use std::collections::BTreeSet;
use std::fmt;

pub fn to_utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flags {
    pub global: bool,
    pub ignore_case: bool,
    pub multiline: bool,
}

impl Flags {
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut f = Flags {
            global: false,
            ignore_case: false,
            multiline: false,
        };
        for c in s.chars() {
            match c {
                'g' => f.global = true,
                'i' => f.ignore_case = true,
                'm' => f.multiline = true,
                _ => return Err(format!("invalid regex flag {:?}", c)),
            }
        }
        Ok(f)
    }
}

impl fmt::Display for Flags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.global {
            f.write_str("g")?;
        }
        if self.ignore_case {
            f.write_str("i")?;
        }
        if self.multiline {
            f.write_str("m")?;
        }
        Ok(())
    }
}

pub type Pattern = Vec<Item>;

type CaptureSlots = Vec<Option<(usize, usize)>>;
type State = (usize, CaptureSlots);

#[derive(Clone, Debug)]
pub enum Item {
    Atom(Atom),
    Group(u32, Pattern),
    NonCapture(Pattern),
    Alt(Vec<Pattern>),
    Quant(Box<Pattern>, u32, u32, bool), // min, max, greedy
}

#[derive(Clone, Debug)]
pub enum Atom {
    Literal(Vec<u16>),
    Any,
    Class(ClassAtom),
    Anchor(Anchor),
    Backref(u32),
}

#[derive(Clone, Debug)]
pub enum ClassAtom {
    Ranges {
        ranges: Vec<(u16, u16)>,
        negated: bool,
    },
    Folded {
        members: BTreeSet<u16>,
        negated: bool,
    },
}

#[derive(Clone, Debug)]
pub enum Anchor {
    StartOfLine,
    EndOfLine,
    WordBoundary,
    NonWordBoundary,
}

#[derive(Clone, Debug)]
pub struct Match {
    pub captures: Vec<Option<(usize, usize)>>, // UTF-16 code-unit indices
}

#[derive(Clone, Debug)]
pub struct Regex {
    source: String,
    flags: Flags,
    pattern: Pattern,
    group_count: u32,
    max_steps: usize,
}

impl Regex {
    pub fn new(source: &str, flags: &str) -> Result<Self, String> {
        let flags = Flags::parse(flags)?;
        let mut parser = ReParser::new(source);
        parser.ignore_case = flags.ignore_case;
        let pattern = parser.parse_pattern()?;
        let group_count = parser.group_count;
        Ok(Regex {
            source: source.to_string(),
            flags,
            pattern,
            group_count,
            max_steps: 1_000_000,
        })
    }

    pub fn with_max_steps(mut self, max: usize) -> Self {
        self.max_steps = max;
        self
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn flags(&self) -> &Flags {
        &self.flags
    }

    pub fn group_count(&self) -> u32 {
        self.group_count
    }

    pub fn find(&self, input: &str, start: usize) -> Result<Option<Match>, String> {
        let input = to_utf16(input);
        for pos in start..=input.len() {
            let mut ctx = Ctx::new(&input, self.group_count, self.max_steps);
            ctx.pos = pos;
            if self.match_pattern(&self.pattern, 0, &mut ctx)? {
                let mut captures = ctx.captures;
                if captures[0].is_none() {
                    captures[0] = Some((pos, ctx.pos));
                }
                return Ok(Some(Match { captures }));
            }
        }
        Ok(None)
    }

    pub fn test(&self, input: &str) -> Result<bool, String> {
        Ok(self.find(input, 0)?.is_some())
    }

    pub fn find_all(&self, input: &str) -> Result<Vec<Match>, String> {
        let input16 = to_utf16(input);
        let mut out = Vec::new();
        let mut start = 0usize;
        while start <= input16.len() {
            match self.find_from(&input16, start)? {
                None => break,
                Some(m) => {
                    let (_, end) = m.captures[0].unwrap();
                    if end == start {
                        start += 1;
                    } else {
                        start = end;
                    }
                    out.push(m);
                }
            }
        }
        Ok(out)
    }

    fn find_from(&self, input: &[u16], start: usize) -> Result<Option<Match>, String> {
        for pos in start..=input.len() {
            let mut ctx = Ctx::new(input, self.group_count, self.max_steps);
            ctx.pos = pos;
            if self.match_pattern(&self.pattern, 0, &mut ctx)? {
                let mut captures = ctx.captures;
                if captures[0].is_none() {
                    captures[0] = Some((pos, ctx.pos));
                }
                return Ok(Some(Match { captures }));
            }
        }
        Ok(None)
    }

    pub fn replace(&self, input: &str, replacement: &str) -> Result<String, String> {
        let input16 = to_utf16(input);
        let full = u16_slice_to_str(&input16);
        if self.flags.global {
            let matches = self.find_all_from(&input16)?;
            if matches.is_empty() {
                return Ok(full);
            }
            let mut out = String::new();
            let mut last = 0usize;
            for m in &matches {
                let (a, b) = m.captures[0].unwrap();
                out.push_str(&u16_range_to_str(&input16, last, a));
                out.push_str(&self.expand_replacement(replacement, &input16, m)?);
                last = b;
            }
            out.push_str(&u16_range_to_str(&input16, last, input16.len()));
            Ok(out)
        } else {
            match self.find_from(&input16, 0)? {
                None => Ok(full),
                Some(m) => {
                    let (a, b) = m.captures[0].unwrap();
                    let mut out = String::new();
                    out.push_str(&u16_range_to_str(&input16, 0, a));
                    out.push_str(&self.expand_replacement(replacement, &input16, &m)?);
                    out.push_str(&u16_range_to_str(&input16, b, input16.len()));
                    Ok(out)
                }
            }
        }
    }

    pub fn split(
        &self,
        input: &str,
        limit: Option<usize>,
    ) -> Result<Vec<String>, String> {
        let input16 = to_utf16(input);
        let mut out = Vec::new();
        let mut last = 0usize;
        let mut start = 0usize;
        let lim = limit.unwrap_or(usize::MAX);
        while out.len() < lim && start <= input16.len() {
            match self.find_from(&input16, start)? {
                None => break,
                Some(m) => {
                    let (a, b) = m.captures[0].unwrap();
                    out.push(u16_range_to_str(&input16, last, a));
                    for i in 1..=self.group_count as usize {
                        if let Some((s, e)) = m.captures.get(i).copied().flatten() {
                            out.push(u16_range_to_str(&input16, s, e));
                        }
                    }
                    if b == start {
                        start += 1;
                    } else {
                        start = b;
                    }
                    last = b;
                }
            }
        }
        out.push(u16_range_to_str(&input16, last, input16.len()));
        if let Some(lim) = limit {
            out.truncate(lim);
        }
        Ok(out)
    }

    fn find_all_from(&self, input: &[u16]) -> Result<Vec<Match>, String> {
        let mut out = Vec::new();
        let mut start = 0usize;
        while start <= input.len() {
            match self.find_from(input, start)? {
                None => break,
                Some(m) => {
                    let (_, end) = m.captures[0].unwrap();
                    if end == start {
                        start += 1;
                    } else {
                        start = end;
                    }
                    out.push(m);
                }
            }
        }
        Ok(out)
    }

    fn expand_replacement(
        &self,
        template: &str,
        input: &[u16],
        m: &Match,
    ) -> Result<String, String> {
        let mut out = String::new();
        let full = m.captures[0].unwrap();
        let before = u16_range_to_str(input, 0, full.0);
        let after = u16_range_to_str(input, full.1, input.len());
        let mut chars = template.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '$' {
                out.push(c);
                continue;
            }
            match chars.peek() {
                Some('$') => {
                    out.push('$');
                    chars.next();
                }
                Some('&') => {
                    out.push_str(&u16_range_to_str(input, full.0, full.1));
                    chars.next();
                }
                Some('\'') => {
                    out.push_str(&after);
                    chars.next();
                }
                Some('`') => {
                    out.push_str(&before);
                    chars.next();
                }
                Some(d) if d.is_ascii_digit() => {
                    let mut n: u32 = 0;
                    while let Some(d) = chars.peek() {
                        if !d.is_ascii_digit() {
                            break;
                        }
                        n = n * 10 + d.to_digit(10).unwrap();
                        chars.next();
                    }
                    if n > 0 && n <= self.group_count {
                        if let Some(Some((s, e))) = m.captures.get(n as usize) {
                            out.push_str(&u16_range_to_str(input, *s, *e));
                        }
                    }
                }
                _ => out.push('$'),
            }
        }
        Ok(out)
    }

    fn match_pattern(
        &self,
        pat: &[Item],
        idx: usize,
        ctx: &mut Ctx,
    ) -> Result<bool, String> {
        if idx >= pat.len() {
            return Ok(true);
        }
        ctx.step()?;
        match &pat[idx] {
            Item::Atom(a) => {
                if self.match_atom(a, ctx)? {
                    self.match_pattern(pat, idx + 1, ctx)
                } else {
                    Ok(false)
                }
            }
            Item::Group(id, body) => {
                let start = ctx.pos;
                let saved_slot = ctx.captures[*id as usize];
                ctx.captures[*id as usize] = Some((start, start));
                if self.match_pattern(body, 0, ctx)? {
                    ctx.captures[*id as usize] = Some((start, ctx.pos));
                    if self.match_pattern(pat, idx + 1, ctx)? {
                        return Ok(true);
                    }
                }
                ctx.captures[*id as usize] = saved_slot;
                Ok(false)
            }
            Item::NonCapture(body) => {
                if self.match_pattern(body, 0, ctx)? {
                    self.match_pattern(pat, idx + 1, ctx)
                } else {
                    Ok(false)
                }
            }
            Item::Alt(alts) => {
                let saved = ctx.save();
                for alt in alts {
                    if self.match_pattern(alt, 0, ctx)? {
                        return Ok(true);
                    }
                    ctx.restore(&saved);
                }
                Ok(false)
            }
            Item::Quant(body, min, max, greedy) => {
                self.match_quant(body, *min, *max, *greedy, &pat[idx + 1..], ctx)
            }
        }
    }

    fn match_quant(
        &self,
        body: &Pattern,
        min: u32,
        max: u32,
        greedy: bool,
        rest: &[Item],
        ctx: &mut Ctx,
    ) -> Result<bool, String> {
        let max = max.min((ctx.input.len() - ctx.pos) as u32 + min);
        let mut states: Vec<State> = vec![(ctx.pos, ctx.captures.clone())];
        let mut count = 0u32;
        loop {
            if count >= max {
                break;
            }
            let before = ctx.pos;
            let saved = ctx.save();
            if !self.match_pattern(body, 0, ctx)? {
                ctx.restore(&saved);
                break;
            }
            count += 1;
            states.push((ctx.pos, ctx.captures.clone()));
            if ctx.pos == before && count >= min {
                break;
            }
        }
        let iter: Box<dyn Iterator<Item = &State>> = if greedy {
            Box::new(states[min as usize..].iter().rev())
        } else {
            Box::new(states[min as usize..].iter())
        };
        for (pos, caps) in iter {
            ctx.pos = *pos;
            ctx.captures.clone_from(caps);
            if self.match_pattern(rest, 0, ctx)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn match_atom(&self, atom: &Atom, ctx: &mut Ctx) -> Result<bool, String> {
        ctx.step()?;
        match atom {
            Atom::Literal(v) => {
                if ctx.pos + v.len() <= ctx.input.len() {
                    if self.flags.ignore_case {
                        let a = u16_slice_to_str(&ctx.input[ctx.pos..ctx.pos + v.len()]);
                        let b = u16_slice_to_str(v);
                        if caseless::default_case_fold_str(&a)
                            == caseless::default_case_fold_str(&b)
                        {
                            ctx.pos += v.len();
                            Ok(true)
                        } else {
                            Ok(false)
                        }
                    } else if ctx.input[ctx.pos..].starts_with(v) {
                        ctx.pos += v.len();
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                } else {
                    Ok(false)
                }
            }
            Atom::Any => {
                if ctx.pos < ctx.input.len() && !is_line_terminator(ctx.input[ctx.pos]) {
                    ctx.pos += 1;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Atom::Class(ca) => {
                if ctx.pos >= ctx.input.len() {
                    return Ok(false);
                }
                let u = ctx.input[ctx.pos];
                let matched = match ca {
                    ClassAtom::Ranges { ranges, negated } => {
                        let in_range = ranges.iter().any(|&(a, b)| u >= a && u <= b);
                        in_range != *negated
                    }
                    ClassAtom::Folded { members, negated } => {
                        let folded = fold_first(u);
                        members.contains(&folded) != *negated
                    }
                };
                if matched {
                    ctx.pos += 1;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Atom::Anchor(a) => match a {
                Anchor::StartOfLine => {
                    if ctx.pos == 0
                        || (self.flags.multiline && is_line_terminator(ctx.input[ctx.pos - 1]))
                    {
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                }
                Anchor::EndOfLine => {
                    if ctx.pos == ctx.input.len()
                        || (self.flags.multiline && is_line_terminator(ctx.input[ctx.pos]))
                    {
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                }
                Anchor::WordBoundary => Ok(is_word_boundary(ctx.input, ctx.pos)),
                Anchor::NonWordBoundary => Ok(!is_word_boundary(ctx.input, ctx.pos)),
            },
            Atom::Backref(id) => {
                if let Some(Some((s, e))) = ctx.captures.get(*id as usize) {
                    let cap = &ctx.input[*s..*e];
                    if ctx.pos + cap.len() <= ctx.input.len() {
                        if self.flags.ignore_case {
                            let a = u16_slice_to_str(cap);
                            let b = u16_slice_to_str(
                                &ctx.input[ctx.pos..ctx.pos + cap.len()]);
                            if caseless::default_case_fold_str(&a)
                                == caseless::default_case_fold_str(&b)
                            {
                                ctx.pos += cap.len();
                                Ok(true)
                            } else {
                                Ok(false)
                            }
                        } else if ctx.input[ctx.pos..].starts_with(cap) {
                            ctx.pos += cap.len();
                            Ok(true)
                        } else {
                            Ok(false)
                        }
                    } else {
                        Ok(false)
                    }
                } else {
                    Ok(true)
                }
            }
        }
    }
}

struct Ctx<'a> {
    input: &'a [u16],
    pos: usize,
    captures: Vec<Option<(usize, usize)>>,
    steps: usize,
    max_steps: usize,
}

impl<'a> Ctx<'a> {
    fn new(input: &'a [u16], group_count: u32, max_steps: usize) -> Self {
        Ctx {
            input,
            pos: 0,
            captures: vec![None; group_count as usize + 1],
            steps: 0,
            max_steps,
        }
    }

    fn step(&mut self) -> Result<(), String> {
        self.steps += 1;
        if self.steps > self.max_steps {
            return Err("regex match step limit exceeded".to_string());
        }
        Ok(())
    }

    fn save(&self) -> CtxState {
        CtxState {
            pos: self.pos,
            captures: self.captures.clone(),
            steps: self.steps,
        }
    }

    fn restore(&mut self, s: &CtxState) {
        self.pos = s.pos;
        self.captures.clone_from(&s.captures);
        self.steps = s.steps;
    }
}

#[derive(Clone)]
struct CtxState {
    pos: usize,
    captures: Vec<Option<(usize, usize)>>,
    steps: usize,
}

fn u16_slice_to_str(v: &[u16]) -> String {
    char::decode_utf16(v.iter().copied())
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect()
}

fn u16_range_to_str(v: &[u16], start: usize, end: usize) -> String {
    let start = start.min(v.len());
    let end = end.min(v.len());
    if start >= end {
        return String::new();
    }
    u16_slice_to_str(&v[start..end])
}

fn is_line_terminator(u: u16) -> bool {
    u == 0x000A || u == 0x000D || u == 0x2028 || u == 0x2029
}

fn is_word_char(u: u16) -> bool {
    matches!(u,
        0x0030..=0x0039 | 0x0041..=0x005A | 0x0061..=0x007A | 0x005F)
}

fn is_word_boundary(input: &[u16], pos: usize) -> bool {
    let left = pos > 0 && is_word_char(input[pos - 1]);
    let right = pos < input.len() && is_word_char(input[pos]);
    left != right
}

fn fold_first(u: u16) -> u16 {
    if let Some(c) = char::from_u32(u as u32) {
        let folded = caseless::default_case_fold_str(&c.to_string());
        folded.encode_utf16().next().unwrap_or(u)
    } else {
        u
    }
}

struct ReParser {
    src: Vec<char>,
    pos: usize,
    group_count: u32,
    ignore_case: bool,
}

impl ReParser {
    fn new(source: &str) -> Self {
        ReParser {
            src: source.chars().collect(),
            pos: 0,
            group_count: 0,
            ignore_case: false,
        }
    }

    fn parse_pattern(&mut self) -> Result<Pattern, String> {
        let mut alts = Vec::new();
        alts.push(self.parse_seq(&['|', ')'])?);
        while self.peek() == Some('|') {
            self.advance();
            alts.push(self.parse_seq(&['|', ')'])?);
        }
        if alts.len() == 1 {
            Ok(alts.into_iter().next().unwrap())
        } else {
            Ok(vec![Item::Alt(alts)])
        }
    }

    fn parse_seq(&mut self, stop: &[char]) -> Result<Pattern, String> {
        let mut seq = Vec::new();
        loop {
            if self.at_end() {
                break;
            }
            if stop.contains(&self.peek().unwrap()) {
                break;
            }
            let atom = self.parse_atom()?;
            let item = if let Some(q) = self.parse_quantifier()? {
                Item::Quant(Box::new(vec![atom]), q.0, q.1, q.2)
            } else {
                atom
            };
            seq.push(item);
        }
        Ok(seq)
    }

    fn parse_atom(&mut self) -> Result<Item, String> {
        if self.at_end() {
            return Err("unexpected end of pattern".to_string());
        }
        match self.peek().unwrap() {
            '(' => {
                self.advance();
                if self.peek() == Some('?') && self.peek_at(1) == Some(':') {
                    self.advance();
                    self.advance();
                    let body = self.parse_seq(&[')'])?;
                    if self.peek() != Some(')') {
                        return Err("unterminated non-capturing group".to_string());
                    }
                    self.advance();
                    Ok(Item::NonCapture(body))
                } else {
                    self.group_count += 1;
                    let id = self.group_count;
                    let body = self.parse_seq(&[')'])?;
                    if self.peek() != Some(')') {
                        return Err("unterminated capturing group".to_string());
                    }
                    self.advance();
                    Ok(Item::Group(id, body))
                }
            }
            '[' => self.parse_class(),
            '^' => {
                self.advance();
                Ok(Item::Atom(Atom::Anchor(Anchor::StartOfLine)))
            }
            '$' => {
                self.advance();
                Ok(Item::Atom(Atom::Anchor(Anchor::EndOfLine)))
            }
            '.' => {
                self.advance();
                Ok(Item::Atom(Atom::Any))
            }
            '\\' => {
                self.advance();
                let esc = self.parse_escape(false)?;
                Ok(self.escape_to_item(esc))
            }
            c => {
                self.advance();
                let v = char_to_utf16(c);
                Ok(Item::Atom(Atom::Literal(v)))
            }
        }
    }

    fn parse_class(&mut self) -> Result<Item, String> {
        self.advance(); // '['
        let mut negated = false;
        if self.peek() == Some('^') {
            negated = true;
            self.advance();
        }
        let mut ranges: Vec<(u16, u16)> = Vec::new();
        let mut literals: Vec<u16> = Vec::new();
        let mut pending_start: Option<u16> = None;
        while !self.at_end() && self.peek() != Some(']') {
            let piece = self.parse_class_piece()?;
            match piece {
                ClassPiece::Code(code) => {
                    if self.peek() == Some('-') && self.peek_at(1) != Some(']') {
                        // This code is the left side of a range.
                        if let Some(p) = pending_start.take() {
                            literals.push(p);
                        }
                        pending_start = Some(code);
                        self.advance(); // '-'
                    } else if let Some(start) = pending_start {
                        if start > code {
                            return Err("invalid character class range".to_string());
                        }
                        ranges.push((start, code));
                        pending_start = None;
                    } else {
                        literals.push(code);
                    }
                }
                ClassPiece::Ranges(rs) => {
                    if let Some(p) = pending_start.take() {
                        literals.push(p);
                    }
                    ranges.extend(rs);
                }
            }
        }
        if self.at_end() {
            return Err("unterminated character class".to_string());
        }
        if let Some(p) = pending_start.take() {
            literals.push(p);
            literals.push(0x002D); // literal '-'
        }
        self.advance(); // ']'
        for &lit in &literals {
            ranges.push((lit, lit));
        }
        ranges = normalize_ranges(ranges);
        let class = if self.ignore_case {
            let mut members = BTreeSet::new();
            for &(a, b) in &ranges {
                for u in a..=b {
                    members.insert(fold_first(u));
                }
            }
            ClassAtom::Folded { members, negated }
        } else {
            ClassAtom::Ranges { ranges, negated }
        };
        Ok(Item::Atom(Atom::Class(class)))
    }

    fn parse_class_piece(&mut self) -> Result<ClassPiece, String> {
        if self.at_end() {
            return Err("unterminated character class".to_string());
        }
        let c = self.peek().unwrap();
        match c {
            '\\' => {
                self.advance();
                match self.parse_escape(true)? {
                    Escape::Code(u) => Ok(ClassPiece::Code(u)),
                    Escape::Class(cr, neg) => {
                        let rs = if neg { complement_ranges(&cr) } else { cr };
                        Ok(ClassPiece::Ranges(normalize_ranges(rs)))
                    }
                    Escape::Anchor(_) => Ok(ClassPiece::Code(0x0008)), // \b inside class is backspace
                    Escape::Decimal(v) => {
                        if v <= 0xFFFF {
                            Ok(ClassPiece::Code(v as u16))
                        } else {
                            Err("decimal escape value too large in class".to_string())
                        }
                    }
                }
            }
            c => {
                self.advance();
                Ok(ClassPiece::Code(char_to_utf16_single(c)?))
            }
        }
    }

    fn parse_escape(&mut self, in_class: bool) -> Result<Escape, String> {
        if self.at_end() {
            return Err("unexpected end of pattern after \\".to_string());
        }
        let c = self.peek().unwrap();
        match c {
            'f' => {
                self.advance();
                Ok(Escape::Code(0x000C))
            }
            'n' => {
                self.advance();
                Ok(Escape::Code(0x000A))
            }
            'r' => {
                self.advance();
                Ok(Escape::Code(0x000D))
            }
            't' => {
                self.advance();
                Ok(Escape::Code(0x0009))
            }
            'v' => {
                self.advance();
                Ok(Escape::Code(0x000B))
            }
            'b' if !in_class => {
                self.advance();
                Ok(Escape::Anchor(Anchor::WordBoundary))
            }
            'b' => {
                self.advance();
                Ok(Escape::Code(0x0008))
            }
            'B' => {
                self.advance();
                Ok(Escape::Anchor(Anchor::NonWordBoundary))
            }
            'd' => {
                self.advance();
                Ok(Escape::Class(vec![(0x0030, 0x0039)], false))
            }
            'D' => {
                self.advance();
                Ok(Escape::Class(vec![(0x0030, 0x0039)], true))
            }
            's' => {
                self.advance();
                Ok(Escape::Class(
                    vec![(0x0009, 0x0009), (0x000A, 0x000A), (0x000C, 0x000C), (0x000D, 0x000D), (0x0020, 0x0020), (0x00A0, 0x00A0), (0xFEFF, 0xFEFF)],
                    false,
                ))
            }
            'S' => {
                self.advance();
                Ok(Escape::Class(
                    vec![(0x0009, 0x0009), (0x000A, 0x000A), (0x000C, 0x000C), (0x000D, 0x000D), (0x0020, 0x0020), (0x00A0, 0x00A0), (0xFEFF, 0xFEFF)],
                    true,
                ))
            }
            'w' => {
                self.advance();
                Ok(Escape::Class(
                    vec![
                        (0x0030, 0x0039),
                        (0x0041, 0x005A),
                        (0x005F, 0x005F),
                        (0x0061, 0x007A),
                    ],
                    false,
                ))
            }
            'W' => {
                self.advance();
                Ok(Escape::Class(
                    vec![
                        (0x0030, 0x0039),
                        (0x0041, 0x005A),
                        (0x005F, 0x005F),
                        (0x0061, 0x007A),
                    ],
                    true,
                ))
            }
            'c' => {
                self.advance();
                if self.at_end() {
                    return Err("unterminated control escape".to_string());
                }
                let ch = self.peek().unwrap();
                self.advance();
                let code = if ch.is_ascii_alphabetic() {
                    (ch as u32 & 0x1F) as u16
                } else {
                    (ch as u32 % 32) as u16
                };
                Ok(Escape::Code(code))
            }
            'x' => {
                self.advance();
                Ok(Escape::Code(self.parse_hex(2)?))
            }
            'u' => {
                self.advance();
                Ok(Escape::Code(self.parse_hex(4)?))
            }
            '0'..='9' => {
                let mut value: u32 = 0;
                while let Some(d) = self.peek() {
                    if !d.is_ascii_digit() {
                        break;
                    }
                    value = value * 10 + d.to_digit(10).unwrap();
                    self.advance();
                }
                Ok(Escape::Decimal(value))
            }
            c => {
                self.advance();
                Ok(Escape::Code(char_to_utf16_single(c)?))
            }
        }
    }

    fn parse_quantifier(&mut self) -> Result<Option<(u32, u32, bool)>, String> {
        if self.at_end() {
            return Ok(None);
        }
        match self.peek().unwrap() {
            '*' => {
                self.advance();
                let greedy = !self.eat('?');
                Ok(Some((0, u32::MAX, greedy)))
            }
            '+' => {
                self.advance();
                let greedy = !self.eat('?');
                Ok(Some((1, u32::MAX, greedy)))
            }
            '?' => {
                self.advance();
                let greedy = !self.eat('?');
                Ok(Some((0, 1, greedy)))
            }
            '{' => {
                self.advance();
                let min = self.parse_number()?;
                let mut max = min;
                if self.eat(',') {
                    if self.peek() == Some('}') {
                        max = u32::MAX;
                    } else {
                        max = self.parse_number()?;
                    }
                }
                if self.peek() != Some('}') {
                    return Err("unterminated quantifier".to_string());
                }
                self.advance();
                let greedy = !self.eat('?');
                Ok(Some((min, max, greedy)))
            }
            _ => Ok(None),
        }
    }

    fn parse_hex(&mut self, digits: usize) -> Result<u16, String> {
        let mut value: u32 = 0;
        for _ in 0..digits {
            if self.at_end() {
                return Err("invalid hex escape".to_string());
            }
            let c = self.peek().unwrap();
            self.advance();
            value = value * 16
                + c.to_digit(16)
                    .ok_or_else(|| "invalid hex escape".to_string())?;
        }
        Ok(value as u16)
    }

    fn parse_number(&mut self) -> Result<u32, String> {
        if self.at_end() || !self.peek().unwrap().is_ascii_digit() {
            return Err("expected number".to_string());
        }
        let mut value: u32 = 0;
        while let Some(d) = self.peek() {
            if !d.is_ascii_digit() {
                break;
            }
            value = value * 10 + d.to_digit(10).unwrap();
            self.advance();
        }
        Ok(value)
    }

    fn peek(&self) -> Option<char> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.src.get(self.pos + offset).copied()
    }

    fn advance(&mut self) {
        if !self.at_end() {
            self.pos += 1;
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.advance();
            true
        } else {
            false
        }
    }
}

#[derive(Clone)]
enum Escape {
    Code(u16),
    Class(Vec<(u16, u16)>, bool),
    Anchor(Anchor),
    Decimal(u32),
}

enum ClassPiece {
    Code(u16),
    Ranges(Vec<(u16, u16)>),
}

impl ReParser {
    fn escape_to_item(&mut self, esc: Escape) -> Item {
        match esc {
            Escape::Code(u) => Item::Atom(Atom::Literal(vec![u])),
            Escape::Class(ranges, negated) => {
                let ranges = normalize_ranges(ranges);
                let class = if self.ignore_case {
                    let mut members = BTreeSet::new();
                    for (a, b) in ranges {
                        for u in a..=b {
                            members.insert(fold_first(u));
                        }
                    }
                    ClassAtom::Folded { members, negated }
                } else {
                    ClassAtom::Ranges { ranges, negated }
                };
                Item::Atom(Atom::Class(class))
            }
            Escape::Anchor(a) => Item::Atom(Atom::Anchor(a)),
            Escape::Decimal(v) => {
                if v == 0 {
                    Item::Atom(Atom::Literal(vec![0]))
                } else if v > 0 && v <= 99 {
                    // Treat as a backreference; the matcher ignores ids beyond group_count.
                    Item::Atom(Atom::Backref(v))
                } else {
                    Item::Atom(Atom::Literal(vec![v as u16]))
                }
            }
        }
    }
}

fn char_to_utf16_single(c: char) -> Result<u16, String> {
    let mut buf = [0u16; 2];
    let encoded = c.encode_utf16(&mut buf);
    if encoded.len() == 1 {
        Ok(encoded[0])
    } else {
        Err("supplementary character in class literal not supported".to_string())
    }
}

fn char_to_utf16(c: char) -> Vec<u16> {
    let mut buf = [0u16; 2];
    let len = c.encode_utf16(&mut buf).len();
    buf[..len].to_vec()
}

fn normalize_ranges(mut ranges: Vec<(u16, u16)>) -> Vec<(u16, u16)> {
    if ranges.is_empty() {
        return ranges;
    }
    ranges.sort_by_key(|r| r.0);
    let mut out = Vec::new();
    let mut cur = ranges[0];
    for (a, b) in ranges.into_iter().skip(1) {
        if a <= cur.1.saturating_add(1) {
            cur.1 = cur.1.max(b);
        } else {
            out.push(cur);
            cur = (a, b);
        }
    }
    out.push(cur);
    out
}

fn complement_ranges(ranges: &[(u16, u16)]) -> Vec<(u16, u16)> {
    let full: Vec<(u16, u16)> = vec![(0, u16::MAX)];
    subtract_ranges(&full, ranges)
}

fn subtract_ranges(a: &[(u16, u16)], b: &[(u16, u16)]) -> Vec<(u16, u16)> {
    let mut out = Vec::new();
    let mut bi = 0usize;
    for &(mut a0, a1) in a {
        while bi < b.len() && b[bi].1 < a0 {
            bi += 1;
        }
        let mut i = bi;
        while i < b.len() && b[i].0 <= a1 {
            let (b0, b1) = b[i];
            if b0 > a0 {
                out.push((a0, b0 - 1));
            }
            a0 = b1.saturating_add(1);
            if a0 > a1 {
                break;
            }
            i += 1;
        }
        if a0 <= a1 {
            out.push((a0, a1));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_conversion() {
        assert_eq!(to_utf16("a中😀"), vec![0x0061, 0x4E2D, 0xD83D, 0xDE00]);
    }

    #[test]
    fn flags_parse() {
        let f = Flags::parse("gi").unwrap();
        assert!(f.global && f.ignore_case && !f.multiline);
        assert!(Flags::parse("x").is_err());
    }

    #[test]
    fn match_literals_and_any() {
        let re = Regex::new("a.c", "").unwrap();
        let m = re.find("abc", 0).unwrap().unwrap();
        assert_eq!(m.captures[0], Some((0, 3)));
        assert!(re.find("ab", 0).unwrap().is_none());
    }

    #[test]
    fn match_alternation() {
        let re = Regex::new("cat|dog", "").unwrap();
        assert!(re.test("dog").unwrap());
        assert!(!re.test("cow").unwrap());
    }

    #[test]
    fn match_quantifiers() {
        let re = Regex::new("a+b*", "").unwrap();
        assert!(re.test("aaabbb").unwrap());
        assert!(re.test("aaab").unwrap());
        assert!(!re.test("bbb").unwrap());
        assert!(!re.test("ccc").unwrap());
    }

    #[test]
    fn match_captures() {
        let re = Regex::new("([a-z]+)-(\\d+)", "").unwrap();
        let m = re.find("id-42", 0).unwrap().unwrap();
        assert_eq!(m.captures[0], Some((0, 5)));
        assert_eq!(m.captures[1], Some((0, 2)));
        assert_eq!(m.captures[2], Some((3, 5)));
    }

    #[test]
    fn match_backref() {
        let re = Regex::new(r"(.)\1", "").unwrap();
        assert!(re.test("aa").unwrap());
        assert!(!re.test("ab").unwrap());
    }

    #[test]
    fn replace_with_capture() {
        let re = Regex::new(r"a(\d)", "").unwrap();
        assert_eq!(re.replace("a1 a2", "x$1").unwrap(), "x1 a2");
    }

    #[test]
    fn replace_global() {
        let re = Regex::new(r"a(\d)", "g").unwrap();
        assert_eq!(re.replace("a1 a2", "x$1").unwrap(), "x1 x2");
    }

    #[test]
    fn split_with_regex() {
        let re = Regex::new(r"[,;]\s*", "").unwrap();
        let parts = re.split("a, b;c", None).unwrap();
        assert_eq!(parts, vec!["a", "b", "c"]);
    }

    #[test]
    fn split_captures() {
        let re = Regex::new(r"(-)", "").unwrap();
        let parts = re.split("a-b", None).unwrap();
        assert_eq!(parts, vec!["a", "-", "b"]);
    }

    #[test]
    fn case_insensitive() {
        let re = Regex::new("[a-z]+", "i").unwrap();
        assert!(re.test("ABC").unwrap());
    }

    #[test]
    fn multiline_anchor() {
        let re = Regex::new("^b", "m").unwrap();
        assert!(re.test("a\nb").unwrap());
        let re2 = Regex::new("^b", "").unwrap();
        assert!(!re2.test("a\nb").unwrap());
    }

    #[test]
    fn step_limit() {
        let re = Regex::new(r"a+b", "").unwrap().with_max_steps(10);
        assert!(re.test("aaaaaaaaaa").is_err());
    }
}
