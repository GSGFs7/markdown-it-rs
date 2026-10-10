use super::text::{Text, alnum};

fn mail_char(ch: u16) -> bool {
    alnum(ch) || ch <= 127 && b"-!#$%&'*+/=?^_`{|}~".contains(&(ch as u8))
}

pub(super) fn mail_name_end(text: &Text, start: usize, end: usize) -> Option<usize> {
    if !text.at(start, end).is_some_and(mail_char) {
        return None;
    }

    let mut p = start + 1;
    while p < end && p < start + 64 {
        if mail_char(text.units[p])
            || text.units[p] == 0x2e && text.at(p + 1, end).is_some_and(mail_char)
        {
            p += 1;
        } else {
            break;
        }
    }

    Some(p)
}
