//! Wall-clock times, as a clock in some time zone reads them (no offset), in the shapes HTML's
//! `datetime-local` and `time` inputs use: `2026-12-09T20:00` and `20:00`. To the minute: raids
//! are scheduled no finer.

time::serde::format_description!(
    pub minute,
    PrimitiveDateTime,
    "[year]-[month]-[day]T[hour]:[minute]"
);

time::serde::format_description!(pub clock, Time, "[hour]:[minute]");
