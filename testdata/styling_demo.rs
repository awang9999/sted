// Styling demo — open this file with sted to check the highlight colors.
//
//   blue        keywords          (fn, let, if, else, match, for, in,
//                                  mut, pub, struct, return)
//   green       common types      (f64, usize, u128, i8, u8, i16, bool,
//                                  char, Option, Result, String, str, Vec)
//   purple      literals & variants (Some, None, Ok, Err, true, false)
//   steel blue  numbers           (42, 3.14, 50e10, 0x1f, 0b1010, 0o17)
//   amber       character literals ('a', '\'' , 'é')
//
// Nothing below should be colored (see the last comment block).

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
        _ => 0,
    }
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

// --- nothing below should be colored --------------------------------------
// static   — deliberately excluded from the keyword list
// myself   — a different word that contains a keyword
// iffy     — a different word that contains a keyword
// Somex    — a different word that contains a variant
// False    — case-sensitive: the capitalized form
// xtrue    — a different word that contains a literal
// 50E10    — uppercase E: one run, no valid number
// 1f2      — one run, no valid number
// 0b2, 0o8 — invalid binary, invalid octal
// 'a     — unclosed quote: no closing quote, no color
