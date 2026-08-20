pub struct DateParts {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub weekday: i64,
    pub hours: i64,
    pub minutes: i64,
    pub seconds: i64,
    pub millis: i64,
}

pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

pub fn utc_parts(ms: f64) -> DateParts {
    if ms.is_nan() || ms.is_infinite() {
        return DateParts {
            year: 0,
            month: 0,
            day: 0,
            weekday: 0,
            hours: 0,
            minutes: 0,
            seconds: 0,
            millis: 0,
        };
    }
    let days = (ms / 86_400_000.0).floor() as i64;
    let rem = ms - (days as f64) * 86_400_000.0;
    let (year, month, day) = civil_from_days(days);
    let weekday = (days + 4).rem_euclid(7);
    let hours = (rem / 3_600_000.0).floor() as i64;
    let minutes = ((rem / 60_000.0).floor() as i64) % 60;
    let seconds = ((rem / 1_000.0).floor() as i64) % 60;
    let millis = rem.rem_euclid(1_000.0).floor() as i64;
    DateParts {
        year,
        month,
        day,
        weekday,
        hours,
        minutes,
        seconds,
        millis,
    }
}

/// Calendar components of `ms` in the current local timezone (DST-aware).
pub fn local_parts(ms: f64) -> DateParts {
    if ms.is_nan() || ms.is_infinite() {
        return utc_parts(ms);
    }
    utc_parts(ms + crate::tz::offset_ms(ms) as f64)
}

pub fn to_iso_string(ms: f64) -> String {
    if ms.is_nan() {
        return "Invalid Date".to_string();
    }
    let p = utc_parts(ms);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        p.year, p.month, p.day, p.hours, p.minutes, p.seconds, p.millis
    )
}

/// Local ISO-8601 rendering: local wall-clock components followed by the
/// local offset (`±HH:mm`), e.g. `2026-08-20T20:00:00.123+08:00`.
pub fn to_local_iso_string(ms: f64) -> String {
    if ms.is_nan() {
        return "Invalid Date".to_string();
    }
    let p = utc_parts(ms + crate::tz::offset_ms(ms) as f64);
    let off = crate::tz::offset_ms(ms) / 1000;
    let sign = if off < 0 { '-' } else { '+' };
    let abs = off.abs();
    let oh = abs / 3600;
    let om = (abs % 3600) / 60;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}{}{:02}:{:02}",
        p.year, p.month, p.day, p.hours, p.minutes, p.seconds, p.millis, sign, oh, om
    )
}

pub fn make_date_ms(
    year: i64,
    month: i64,
    day: i64,
    hours: i64,
    minutes: i64,
    seconds: i64,
    millis: i64,
) -> f64 {
    let days = days_from_civil(year, month + 1, day);
    (days * 86_400_000 + hours * 3_600_000 + minutes * 60_000 + seconds * 1_000 + millis) as f64
}

/// Parse an ISO-8601 timestamp.
///
/// Accepted forms:
/// * `YYYY-MM-DD`
/// * `YYYY-MM-DD[T| ]HH:MM:SS[.fff]`
///
/// A trailing timezone may be `Z`/`z` (UTC), an explicit offset
/// (`±HH:mm`, `±HHmm`, or `±HH`), or absent. When absent the wall-clock
/// components are interpreted in the current local timezone; when present
/// they are shifted to UTC by that offset. Unparseable input yields `None`.
pub fn parse_iso_date(s: &str) -> Option<f64> {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let year: i64 = s[0..4].parse().ok()?;
    let month: i64 = s[5..7].parse().ok()?;
    let day: i64 = s[8..10].parse().ok()?;
    let mut hours = 0i64;
    let mut minutes = 0i64;
    let mut seconds = 0i64;
    let mut millis = 0i64;
    let mut offset_minutes: Option<i64> = None;
    if b.len() > 10 {
        if b.len() < 19 || (b[10] != b'T' && b[10] != b' ') || b[13] != b':' || b[16] != b':' {
            return None;
        }
        hours = s[11..13].parse().ok()?;
        minutes = s[14..16].parse().ok()?;
        seconds = s[17..19].parse().ok()?;
        let mut i = 19;
        if i < b.len() && b[i] == b'.' {
            i += 1;
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return None;
            }
            let mut frac = s[start..i].to_string();
            while frac.len() < 3 {
                frac.push('0');
            }
            millis = frac[..3].parse().ok()?;
        }
        if i < b.len() {
            offset_minutes = Some(parse_tz_offset(&s[i..])?);
        }
    }
    let naive = make_date_ms(year, month - 1, day, hours, minutes, seconds, millis);
    match offset_minutes {
        // Explicit offset: wall clock is at that offset, shift back to UTC.
        Some(m) => Some(naive - (m * 60_000) as f64),
        // No offset: interpret as local time.
        None => {
            let utc = crate::tz::local_to_utc_ms(
                year,
                month - 1,
                day,
                hours,
                minutes,
                seconds,
                millis,
            )
            .unwrap_or(naive as i64);
            Some(utc as f64)
        }
    }
}

/// Parse a trailing timezone designator into signed offset minutes (east
/// positive). Returns `None` on malformed input.
fn parse_tz_offset(tz: &str) -> Option<i64> {
    if tz == "Z" || tz == "z" {
        return Some(0);
    }
    let b = tz.as_bytes();
    if b.len() < 3 || (b[0] != b'+' && b[0] != b'-') {
        return None;
    }
    let sign: i64 = if b[0] == b'+' { 1 } else { -1 };
    let digits: Vec<u8> = b[1..].iter().filter(|&&c| c != b':').cloned().collect();
    if digits.len() != 2 && digits.len() != 4 {
        return None;
    }
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let hh: i64 = std::str::from_utf8(&digits[..2]).ok()?.parse().ok()?;
    let mm: i64 = if digits.len() == 4 {
        std::str::from_utf8(&digits[2..4]).ok()?.parse().ok()?
    } else {
        0
    };
    if hh > 23 || mm > 59 {
        return None;
    }
    Some(sign * (hh * 60 + mm))
}

/// Howard Hinnant's `days_from_civil` algorithm (public domain): days since
/// 1970-01-01 for a given proleptic Gregorian calendar date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Howard Hinnant's `civil_from_days` algorithm (public domain): inverse of
/// `days_from_civil`.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
