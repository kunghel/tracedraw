//! Kashida: justifying Arabic-script lines by stretching the joins
//! between connected letters instead of only widening the spaces between
//! words (#3827).
//!
//! Which joins may be stretched follows the joining types of the Unicode
//! Standard (ArabicShaping.txt): a join is the link between a letter that
//! connects to the letter after it (dual joining or join causing) and a
//! letter that connects to the one before it (dual or right joining).
//! A letter that never connects forward (alef, dal, reh, waw, hamza and
//! the like) has no join after it, and lam is never pulled away from an
//! alef after it, which the two form one shape with. Each join gets a
//! rank: the commonly preferred places to stretch a word come first
//! (after a typed kashida, after the seen and sad letters, before a
//! final heh or hah, before a final alef, lam or kaf, and so on), and a
//! justified line stretches the best-ranked join of each word.

/// How a character joins its neighbours in Arabic script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Joining {
    /// Connects on both sides.
    Dual,
    /// Connects only to the letter before it (on its right).
    Right,
    /// Makes its neighbours join without being a letter (the kashida).
    Causing,
    /// Takes no part: it neither joins nor breaks a join (marks).
    Transparent,
    /// Breaks the join (spaces, digits, letters of other scripts).
    None,
}

/// The joining type of `c`, for the Arabic, Arabic Supplement and Arabic
/// Extended-A letters (Persian, Urdu and the other languages written in
/// Arabic script included).
pub(crate) fn joining(c: char) -> Joining {
    use Joining::*;
    let u = c as u32;
    match u {
        0x0640 | 0x200D => Causing,
        0x0610..=0x061A
        | 0x064B..=0x065F
        | 0x0670
        | 0x06D6..=0x06DC
        | 0x06DF..=0x06E4
        | 0x06E7
        | 0x06E8
        | 0x06EA..=0x06ED
        | 0x08D3..=0x08E1
        | 0x08E3..=0x08FF => Transparent,
        0x0622..=0x0625
        | 0x0627
        | 0x0629
        | 0x062F..=0x0632
        | 0x0648
        | 0x0671..=0x0673
        | 0x0675..=0x0677
        | 0x0688..=0x0699
        | 0x06C0
        | 0x06C3..=0x06CB
        | 0x06CD
        | 0x06CF
        | 0x06D2
        | 0x06D3
        | 0x06D5
        | 0x06EE
        | 0x06EF
        | 0x0759..=0x075B
        | 0x076B
        | 0x076C
        | 0x0771
        | 0x0773
        | 0x0774
        | 0x0778
        | 0x0779
        | 0x08AA..=0x08AC
        | 0x08AE
        | 0x08B1
        | 0x08B2
        | 0x08B9 => Right,
        0x0620
        | 0x0626
        | 0x0628
        | 0x062A..=0x062E
        | 0x0633..=0x063F
        | 0x0641..=0x0647
        | 0x0649
        | 0x064A
        | 0x066E
        | 0x066F
        | 0x0678..=0x0687
        | 0x069A..=0x06BF
        | 0x06C1
        | 0x06C2
        | 0x06CC
        | 0x06CE
        | 0x06D0
        | 0x06D1
        | 0x06FA..=0x06FC
        | 0x06FF
        | 0x0750..=0x0758
        | 0x075C..=0x076A
        | 0x076D..=0x0770
        | 0x0772
        | 0x0775..=0x0777
        | 0x077A..=0x077F
        | 0x08A0..=0x08A9
        | 0x08AF
        | 0x08B0
        | 0x08B3..=0x08B8
        | 0x08BA..=0x08BD => Dual,
        _ => None,
    }
}

/// Connects to the letter after it.
fn joins_forward(c: char) -> bool {
    matches!(joining(c), Joining::Dual | Joining::Causing)
}

/// Connects to the letter before it.
fn joins_back(c: char) -> bool {
    matches!(
        joining(c),
        Joining::Dual | Joining::Right | Joining::Causing
    )
}

fn is_alef(c: char) -> bool {
    matches!(
        c,
        '\u{0622}'
            | '\u{0623}'
            | '\u{0625}'
            | '\u{0627}'
            | '\u{0671}'
            | '\u{0672}'
            | '\u{0673}'
            | '\u{0675}'
    )
}

fn is_lam(c: char) -> bool {
    matches!(
        c,
        '\u{0644}' | '\u{06B5}'..='\u{06B8}' | '\u{076A}' | '\u{08A6}'
    )
}

/// The rank of stretching the join between `before` and `after` (the
/// letter that follows it in the text), where `after_final` says whether
/// `after` ends its connected group (joins nothing after it); 1 is the
/// best place, larger numbers worse ones, and `None` means the two do not
/// join or must not be pulled apart.
pub(crate) fn rank(before: char, after: char, after_final: bool) -> Option<u8> {
    if !joins_forward(before) || !joins_back(after) {
        return None;
    }
    if is_lam(before) && is_alef(after) {
        return None;
    }
    let u = after as u32;
    if before == '\u{0640}' {
        return Some(1);
    }
    if matches!(
        before as u32,
        0x0633..=0x0636 | 0x069A..=0x069C | 0x06FA | 0x06FB | 0x075C | 0x076D
    ) {
        return Some(2);
    }
    if after_final {
        // Heh, teh marbuta and the hah family.
        if matches!(
            u,
            0x0629 | 0x0647 | 0x06C0..=0x06C3 | 0x06D5 | 0x062C..=0x062E | 0x0681..=0x0687 | 0x06BE
        ) {
            return Some(3);
        }
        // Alef, tah, zah, lam, kaf and gaf.
        if is_alef(after)
            || is_lam(after)
            || matches!(u, 0x0637 | 0x0638 | 0x0643 | 0x06A9..=0x06B4)
        {
            return Some(4);
        }
        // Reh, waw, ain, qaf and feh.
        if matches!(
            u,
            0x0631 | 0x0632 | 0x0691..=0x0699 | 0x0648 | 0x0624 | 0x06C4..=0x06CB | 0x0639 | 0x063A
                | 0x0641 | 0x0642 | 0x06A1..=0x06A8
        ) {
            return Some(5);
        }
        // Yeh.
        if matches!(u, 0x0626 | 0x0649 | 0x064A | 0x06CC..=0x06CE | 0x06D0..=0x06D3) {
            return Some(6);
        }
        return Some(7);
    }
    Some(8)
}

/// The rank of the join after the cluster `text[start..end]` (see
/// [`rank`]): its last letter (marks skipped) and the next letter of
/// `text`; `None` when there is no join there.
pub(crate) fn rank_after_cluster(text: &str, start: usize, end: usize) -> Option<u8> {
    let before = text
        .get(start..end)?
        .chars()
        .rev()
        .find(|c| joining(*c) != Joining::Transparent)?;
    let mut rest = text
        .get(end..)?
        .chars()
        .filter(|c| joining(*c) != Joining::Transparent);
    let after = rest.next()?;
    let after_final = !joins_forward(after) || !rest.next().is_some_and(joins_back);
    rank(before, after, after_final)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_join_by_their_unicode_joining_types() {
        // beh, alef, dal, hamza, a fatha and a space.
        assert_eq!(joining('\u{0628}'), Joining::Dual);
        assert_eq!(joining('\u{0627}'), Joining::Right);
        assert_eq!(joining('\u{062F}'), Joining::Right);
        assert_eq!(joining('\u{0621}'), Joining::None);
        assert_eq!(joining('\u{064E}'), Joining::Transparent);
        assert_eq!(joining(' '), Joining::None);
        // Persian peh, tcheh, keheh, gaf and Farsi yeh join on both sides,
        // jeh only back.
        for c in ['\u{067E}', '\u{0686}', '\u{06A9}', '\u{06AF}', '\u{06CC}'] {
            assert_eq!(joining(c), Joining::Dual, "{c}");
        }
        assert_eq!(joining('\u{0698}'), Joining::Right);
    }

    #[test]
    fn joins_are_ranked_and_broken_joins_have_none() {
        // After alef nothing joins; lam is not pulled from alef.
        assert_eq!(rank('\u{0627}', '\u{0628}', false), None);
        assert_eq!(rank('\u{0644}', '\u{0627}', true), None);
        // Before hamza (which joins nothing) neither.
        assert_eq!(rank('\u{0628}', '\u{0621}', true), None);
        // After seen, before a final heh, before a final alef.
        assert_eq!(rank('\u{0633}', '\u{0644}', false), Some(2));
        assert_eq!(rank('\u{0644}', '\u{0647}', true), Some(3));
        assert_eq!(rank('\u{0628}', '\u{0627}', true), Some(4));
        assert_eq!(rank('\u{0628}', '\u{062A}', false), Some(8));
        assert_eq!(rank('\u{0640}', '\u{0628}', false), Some(1));
    }

    #[test]
    fn the_join_after_a_cluster_skips_its_marks() {
        // "كتاب": kaf-teh joins (teh not final), teh-alef joins (alef
        // final), alef-beh does not.
        let t = "كتاب";
        let at = |i: usize| t.char_indices().nth(i).map(|(b, _)| b).unwrap();
        assert_eq!(rank_after_cluster(t, at(0), at(1)), Some(8));
        assert_eq!(rank_after_cluster(t, at(1), at(2)), Some(4));
        assert_eq!(rank_after_cluster(t, at(2), at(3)), None);
        assert_eq!(rank_after_cluster(t, at(3), t.len()), None);
        // A beh with a fatha: the mark is part of its cluster.
        let t = "بَت";
        assert_eq!(rank_after_cluster(t, 0, 4), Some(7));
    }
}
