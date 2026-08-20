//! Local-timezone support for `Date`.
//!
//! Two operations are needed to implement JavaScript-style local time:
//!
//! * [`offset_ms`] — the UTC offset (east of UTC, in milliseconds) in effect
//!   at a given UTC instant, accounting for daylight saving time.
//! * [`local_to_utc_ms`] — interpret a wall-clock calendar tuple as local time
//!   and return the corresponding UTC epoch milliseconds, normalizing
//!   out-of-range components (e.g. month 12, day 32, hour 25).
//!
//! The implementation is a thin FFI layer over the platform's own timezone
//! database (glibc `localtime_r`/`mktime` on Unix, the Win32 timezone APIs on
//! Windows), so no tzdata is shipped and no large time dependency is pulled
//! in. When the instant or year is outside the platform's supported range the
//! functions fall back to a zero offset / `None`.

/// Return the UTC offset (milliseconds east of UTC) in effect at the given UTC
/// instant. NaN/infinite input yields 0.
pub fn offset_ms(at_utc_ms: f64) -> i64 {
    imp::offset_ms(at_utc_ms)
}

/// Interpret a local wall-clock tuple (month is 0-based) as local time and
/// return the UTC epoch milliseconds. Out-of-range components are normalized
/// by the platform (mirroring `new Date(year, month, ...)` rollover). Returns
/// `None` when the value cannot be represented by the platform.
#[allow(clippy::too_many_arguments)]
pub fn local_to_utc_ms(
    year: i64,
    month0: i64,
    day: i64,
    hours: i64,
    minutes: i64,
    seconds: i64,
    millis: i64,
) -> Option<i64> {
    imp::local_to_utc_ms(year, month0, day, hours, minutes, seconds, millis)
}

#[cfg(unix)]
mod imp {
    pub fn offset_ms(at_utc_ms: f64) -> i64 {
        if at_utc_ms.is_nan() || at_utc_ms.is_infinite() {
            return 0;
        }
        let secs = at_utc_ms.div_euclid(1000.0) as i64;
        let t = secs as libc::time_t;
        let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
        let r = unsafe { libc::localtime_r(&t, &mut tm) };
        if r.is_null() {
            return 0;
        }
        (tm.tm_gmtoff as i64) * 1000
    }

    pub fn local_to_utc_ms(
        year: i64,
        month0: i64,
        day: i64,
        hours: i64,
        minutes: i64,
        seconds: i64,
        millis: i64,
    ) -> Option<i64> {
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        tm.tm_year = (year - 1900) as libc::c_int;
        tm.tm_mon = month0 as libc::c_int;
        tm.tm_mday = day as libc::c_int;
        tm.tm_hour = hours as libc::c_int;
        tm.tm_min = minutes as libc::c_int;
        tm.tm_sec = seconds as libc::c_int;
        tm.tm_isdst = -1;
        // Sentinel so we can detect mktime(-1) failure: on success mktime
        // always recomputes tm_yday to a value in 0..366.
        tm.tm_yday = SENTINEL_YDAY;
        let t = unsafe { libc::mktime(&mut tm) };
        if t == -1 && tm.tm_yday == SENTINEL_YDAY {
            return None;
        }
        Some((t as i64) * 1000 + millis)
    }

    const SENTINEL_YDAY: libc::c_int = -100_000;
}

#[cfg(windows)]
mod imp {
    use std::ptr;
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::Time::{
        GetTimeZoneInformationForYear, SystemTimeToTzSpecificLocalTime,
        TzSpecificLocalTimeToSystemTime, TIME_ZONE_INFORMATION,
    };

    pub fn offset_ms(at_utc_ms: f64) -> i64 {
        if at_utc_ms.is_nan() || at_utc_ms.is_infinite() {
            return 0;
        }
        let p = crate::date::utc_parts(at_utc_ms);
        if !(0..=u16::MAX as i64).contains(&p.year) {
            return 0;
        }
        let tzi = match tzi_for_year(p.year as u16) {
            Some(t) => t,
            None => return 0,
        };
        let utc = to_systemtime(&p);
        let mut local = unsafe { std::mem::zeroed::<SYSTEMTIME>() };
        if unsafe { SystemTimeToTzSpecificLocalTime(&tzi, &utc, &mut local) } == 0 {
            return 0;
        }
        let utc_sec = sys_to_ms(&utc);
        let local_sec = sys_to_ms(&local);
        local_sec - utc_sec
    }

    pub fn local_to_utc_ms(
        year: i64,
        month0: i64,
        day: i64,
        hours: i64,
        minutes: i64,
        seconds: i64,
        millis: i64,
    ) -> Option<i64> {
        // Normalize out-of-range components using the pure-Gregorian path
        // (timezone-independent rollover) before handing a valid SYSTEMTIME
        // to Win32.
        let naive = crate::date::make_date_ms(year, month0, day, hours, minutes, seconds, 0);
        let p = crate::date::utc_parts(naive);
        if !(0..=u16::MAX as i64).contains(&p.year) {
            return None;
        }
        let tzi = tzi_for_year(p.year as u16)?;
        let local = to_systemtime(&p);
        let mut utc = unsafe { std::mem::zeroed::<SYSTEMTIME>() };
        if unsafe { TzSpecificLocalTimeToSystemTime(&tzi, &local, &mut utc) } == 0 {
            return None;
        }
        Some(sys_to_ms(&utc) + millis)
    }

    fn tzi_for_year(year: u16) -> Option<TIME_ZONE_INFORMATION> {
        let mut tzi = unsafe { std::mem::zeroed::<TIME_ZONE_INFORMATION>() };
        if unsafe { GetTimeZoneInformationForYear(year, ptr::null(), &mut tzi) } == 0 {
            None
        } else {
            Some(tzi)
        }
    }

    fn to_systemtime(p: &crate::date::DateParts) -> SYSTEMTIME {
        SYSTEMTIME {
            wYear: p.year as u16,
            wMonth: p.month as u16,
            wDay: p.day as u16,
            wHour: p.hours as u16,
            wMinute: p.minutes as u16,
            wSecond: p.seconds as u16,
            wMilliseconds: 0,
            wDayOfWeek: 0,
        }
    }

    fn sys_to_ms(st: &SYSTEMTIME) -> i64 {
        crate::date::make_date_ms(
            st.wYear as i64,
            (st.wMonth as i64) - 1,
            st.wDay as i64,
            st.wHour as i64,
            st.wMinute as i64,
            st.wSecond as i64,
            0,
        ) as i64
    }
}

#[cfg(not(any(unix, windows)))]
mod imp {
    pub fn offset_ms(_at_utc_ms: f64) -> i64 {
        0
    }

    pub fn local_to_utc_ms(
        year: i64,
        month0: i64,
        day: i64,
        hours: i64,
        minutes: i64,
        seconds: i64,
        millis: i64,
    ) -> Option<i64> {
        Some(crate::date::make_date_ms(
            year, month0, day, hours, minutes, seconds, millis,
        ))
    }
}
