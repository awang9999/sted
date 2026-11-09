/ Styling demo — open this file with sted to check the highlight colors.
//
//   blue        keywords          (fn, let, if, else, match, for, in,
//                                  mut, pub, struct, return)
//   green       common types      (f64, usize, u128, i8, u8, i16, bool,
//                                  char, Option, Result, String, str, Vec)
//   purple      literals & variants (Some, None, Ok, Err, true, false)
//   steel blue  numbers           (42, 3.14, 50e10, 0x1f, 0b1010, 0o17)
//   amber       character literals ('a', '\'' , 'é')
//   teal        lifetime specifiers ('static, '_, 'a_b2)
//   dark green  comments          (// to the end of the line, marker
//                                  included; nothing else colored in one)
//                                  and /* ... */ spanning lines: a
//                                  nested /* inside a /* counts as
//                                  one comment; code resumes after
//                                  the final */
//
// No non-comment colors below (see the last comment block).

pub struct Point {
    x: f64,
    y: f64,
}

fn first_char(s: &str) -> Option<char> {
    s.chars().next()
}

fn char_code(c: char) -> u8 {
    match c {
        'a' => 1,
        'é' => 2,
        '\\' => 3, // the backslash char: the escape stays inside
        _ => 0,
    }
}

fn pairing() -> char {
    let note = "don't panic: 'ok'"; // naive: the apostrophe pairs with the next quote
    note.chars().next().unwrap_or('?')
}

fn lifetime_demo<'a>(s: &'a str) -> &'static str {
    s
}

fn elided(s: &'_ str) -> &'_ str {
    s
}

// The apostrophe below opens no character literal, so the run after it
// is read as a lifetime: a naive-pairing consequence, pinned here.
fn possessive(owner: &str) -> &str {
    &format!("owner's name: {owner}")
}

// Lifetime specifiers in teal, character literals in amber: the chars
// are styled first, so the quotes of the literals are consumed and
// never read as lifetimes — while the unclosed 'a stays a lifetime.
fn lifetime_block<'a, 'b>(
    x: &'a str,
    y: &'b String,
    c: char,
    d: &'static Option<char>,
) -> &'a str {
    let quoted: char = 'q'; // a char literal, not a lifetime
    let elided: &'_ str = x;
    let tag = format!("'{}", quoted); // the ' here: a lifetime run
    let joined = elided.to_owned() + &tag; // a String, so no return lifetime
    drop((c, d, joined));
    x
}

fn describe(points: &Vec<Point>) -> String {
    let mut count: usize = 0;
    let found: bool = true;

    for point in points {
        if point.x == 3.14 {
            count += 1;
        } else if point.y > 0 {
            count += 2;
        }
    }

    if found {
        return String::from("points: 42");
    }
    String::from("none")
}

fn measure() -> Result<u128, String> {
    let big: u128 = 50e10; // scientific
    let small: i8 = 0x1f; // hex
    let bits: u8 = 0b1010; // binary
    let octal: i16 = 0o17; // octal
    Ok(big + small as u128 + bits as u128 + octal as u128)
}

fn comment_demo() -> u128 {
    let n: u128 = 42; // let 'a' 'static 42: keyword, char, lifetime, digit — all comment
    n
}

fn block_comments() -> u128 {
    /* a comment spanning three lines,
       mentioning 42 and a keyword let,
       and a nested /* inner */ comment */ let n: u128 = 7; // resumes here
    n
}

fn main() {
    let point = Point { x: 3.14, y: 0.0 };
    match first_char(describe(&vec![point])) {
        Some(c) => println!("first: {c}"),
        None => println!("empty"),
    }
    match measure() {
        Ok(v) => println!("{v}"),
        Err(e) => println!("{e}"),
    }
}

// --- no non-comment colors below ------------------------------------------
// static   — deliberately excluded from the keyword list
// myself   — a different word that contains a keyword
// iffy     — a different word that contains a keyword
// Somex    — a different word that contains a variant
// False    — case-sensitive: the capitalized form
// xtrue    — a different word that contains a literal
// 50E10    — uppercase E: one run, no valid number
// 1f2      — one run, no valid number
// 0b2, 0o8 — invalid binary, invalid octal
// '\' — unterminated literal: the escape eats the closing quote
