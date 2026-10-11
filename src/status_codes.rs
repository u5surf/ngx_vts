//! Per-status-code counters, `vts_measure_status_codes`.
//!
//! The server-zone counters only keep the status class (`2xx`, `5xx`, …).
//! This directive, the original module's
//! `vhost_traffic_status_measure_status_codes`, asks for individual codes on
//! top of that: either a list of them, or `all`. A status that is not
//! measured is counted as `other`, so the codes of a zone always add up to
//! its requests.
//!
//! The original gives every zone one counter per measured code up front,
//! which for `all` is 500 counters per zone whether or not those codes ever
//! occur. Here a counter is created the first time its code is seen, so a
//! zone only pays for the codes it has actually served.

/// What a request is counted as.
///
/// `Code` sorts before `Other`, so a zone's codes come out in order with
/// `other` last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StatusCodeSlot {
    /// A measured status code.
    Code(u16),
    /// Any status that is not measured.
    Other,
}

impl StatusCodeSlot {
    /// The value of the `code` label, which is also how the slot is keyed in
    /// shared memory.
    pub fn label(self) -> String {
        match self {
            StatusCodeSlot::Code(code) => code.to_string(),
            StatusCodeSlot::Other => "other".to_string(),
        }
    }

    /// Inverse of [`StatusCodeSlot::label`].
    pub fn from_label(label: &str) -> Option<Self> {
        if label == "other" {
            return Some(StatusCodeSlot::Other);
        }
        label.parse().ok().map(StatusCodeSlot::Code)
    }
}

/// The codes `vts_measure_status_codes` asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MeasureStatusCodes {
    /// The directive is absent: nothing is counted per code.
    #[default]
    Off,
    /// `all`: every status from 100 to 599 is measured.
    All,
    /// The listed codes, sorted and without duplicates.
    Listed(Vec<u16>),
}

impl MeasureStatusCodes {
    /// Parse the directive's arguments.
    ///
    /// `all` has to stand alone. Codes have to be 100 to 599. The original
    /// also insists on ascending order, because it binary-searches the list
    /// as given; this sorts it instead, so any configuration the original
    /// accepts is accepted here too.
    pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Self, String> {
        let args: Vec<&str> = args.into_iter().collect();
        if args.is_empty() {
            return Err("no status codes given".to_string());
        }
        if args.contains(&"all") {
            if args.len() > 1 {
                return Err("\"all\" cannot be combined with status codes".to_string());
            }
            return Ok(MeasureStatusCodes::All);
        }

        let mut codes = Vec::with_capacity(args.len());
        for arg in args {
            match arg.parse::<u16>() {
                Ok(code) if (100..=599).contains(&code) => codes.push(code),
                _ => return Err(format!("invalid status code \"{arg}\"")),
            }
        }
        codes.sort_unstable();
        codes.dedup();
        Ok(MeasureStatusCodes::Listed(codes))
    }

    /// What a response with `status` is counted as, or `None` when no codes
    /// are measured.
    pub fn slot(&self, status: u16) -> Option<StatusCodeSlot> {
        let measured = match self {
            MeasureStatusCodes::Off => return None,
            MeasureStatusCodes::All => (100..=599).contains(&status),
            MeasureStatusCodes::Listed(codes) => codes.binary_search(&status).is_ok(),
        };
        Some(if measured {
            StatusCodeSlot::Code(status)
        } else {
            StatusCodeSlot::Other
        })
    }

    /// Where a stored count belongs under this configuration.  A worker
    /// that a reload has not replaced yet still counts by the old one, so a
    /// code can turn up that this one does not measure; it goes to `other`,
    /// as this configuration would have counted it.
    pub fn reclassify(&self, slot: StatusCodeSlot) -> StatusCodeSlot {
        match slot {
            StatusCodeSlot::Code(code) => self.slot(code).unwrap_or(StatusCodeSlot::Other),
            StatusCodeSlot::Other => StatusCodeSlot::Other,
        }
    }

    /// Identifies the configuration, so that the shared zone, which a
    /// reload keeps, can tell whether its counts were made under the
    /// configuration that is now loaded.  FNV-1a, so it does not depend on
    /// a hasher's seed.
    pub fn fingerprint(&self) -> u64 {
        let (tag, codes): (u8, &[u16]) = match self {
            MeasureStatusCodes::Off => (0, &[]),
            MeasureStatusCodes::All => (1, &[]),
            MeasureStatusCodes::Listed(codes) => (2, codes),
        };
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let bytes = std::iter::once(tag).chain(codes.iter().flat_map(|code| code.to_le_bytes()));
        for byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reclassify_moves_codes_no_longer_measured_to_other() {
        let listed = MeasureStatusCodes::Listed(vec![404]);
        assert_eq!(
            listed.reclassify(StatusCodeSlot::Code(404)),
            StatusCodeSlot::Code(404)
        );
        assert_eq!(
            listed.reclassify(StatusCodeSlot::Code(200)),
            StatusCodeSlot::Other
        );
        assert_eq!(
            listed.reclassify(StatusCodeSlot::Other),
            StatusCodeSlot::Other
        );
        assert_eq!(
            MeasureStatusCodes::All.reclassify(StatusCodeSlot::Code(200)),
            StatusCodeSlot::Code(200)
        );
    }

    #[test]
    fn fingerprint_tells_configurations_apart() {
        let configs = [
            MeasureStatusCodes::Off,
            MeasureStatusCodes::All,
            MeasureStatusCodes::Listed(vec![]),
            MeasureStatusCodes::Listed(vec![200]),
            MeasureStatusCodes::Listed(vec![404]),
            MeasureStatusCodes::Listed(vec![200, 404]),
        ];
        for (i, a) in configs.iter().enumerate() {
            assert_eq!(a.fingerprint(), a.clone().fingerprint());
            for b in &configs[i + 1..] {
                assert_ne!(a.fingerprint(), b.fingerprint(), "{a:?} vs {b:?}");
            }
        }
    }

    #[test]
    fn parse_all() {
        assert_eq!(
            MeasureStatusCodes::parse(["all"]),
            Ok(MeasureStatusCodes::All)
        );
    }

    #[test]
    fn parse_sorts_and_dedups_codes() {
        assert_eq!(
            MeasureStatusCodes::parse(["404", "200", "500", "404"]),
            Ok(MeasureStatusCodes::Listed(vec![200, 404, 500]))
        );
    }

    #[test]
    fn parse_rejects_what_the_original_rejects() {
        for args in [
            vec![],
            vec!["99"],
            vec!["600"],
            vec!["0"],
            vec!["2xx"],
            vec!["-200"],
            vec!["all", "200"],
            vec!["200", "all"],
        ] {
            assert!(
                MeasureStatusCodes::parse(args.iter().copied()).is_err(),
                "{args:?} was accepted"
            );
        }
    }

    #[test]
    fn off_measures_nothing() {
        assert_eq!(MeasureStatusCodes::Off.slot(200), None);
    }

    #[test]
    fn listed_codes_are_measured_and_the_rest_is_other() {
        let measure = MeasureStatusCodes::Listed(vec![200, 404]);
        assert_eq!(measure.slot(200), Some(StatusCodeSlot::Code(200)));
        assert_eq!(measure.slot(404), Some(StatusCodeSlot::Code(404)));
        assert_eq!(measure.slot(500), Some(StatusCodeSlot::Other));
    }

    #[test]
    fn all_measures_every_valid_code() {
        let measure = MeasureStatusCodes::All;
        assert_eq!(measure.slot(100), Some(StatusCodeSlot::Code(100)));
        assert_eq!(measure.slot(599), Some(StatusCodeSlot::Code(599)));
        assert_eq!(measure.slot(99), Some(StatusCodeSlot::Other));
        assert_eq!(measure.slot(600), Some(StatusCodeSlot::Other));
    }

    #[test]
    fn label_round_trips() {
        for slot in [StatusCodeSlot::Code(404), StatusCodeSlot::Other] {
            assert_eq!(StatusCodeSlot::from_label(&slot.label()), Some(slot));
        }
        assert_eq!(StatusCodeSlot::from_label("4xx"), None);
    }

    #[test]
    fn codes_sort_before_other() {
        assert!(StatusCodeSlot::Code(599) < StatusCodeSlot::Other);
    }
}
