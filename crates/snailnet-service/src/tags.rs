//! What a fast-path [`Tag`] means here: the packet's app and, while flows
//! are being named (a site rule exists), its site. The fast path counts
//! bytes per tag without looking inside; the [`crate::hub`] turns the
//! counts back into per-family and per-site rates.
//!
//! Layout: the low 24 bits are the [`AppId`] (`0xFF_FFFF`: not known), the
//! high 8 bits the site: 0 none, 1–253 a slot in the shaper's site list,
//! 254 an HTTPS flow that couldn't be identified (PLAN.md §8.4). 255 never
//! occurs except in [`NO_TAG`], which is what an unknown app with no site
//! packs to, so tags without sites are the plain app ids they always were.

use snailnet_core::classify::AppId;
use snailnet_engine::fastpath::{NO_TAG, Tag};

const APP_MASK: u32 = (1 << 24) - 1;
const UNIDENTIFIED: u32 = 0xFE;

/// Sites with a slot of their own; later ones count as no site.
pub const MAX_SITES: usize = 253;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Site {
    /// Not named, not a known site, or naming is off.
    None,
    /// An HTTPS flow whose name was hidden (ECH) or missing.
    Unidentified,
    /// Index into the site list (below [`MAX_SITES`]).
    Slot(u8),
}

/// The tag for a packet of `app` (`None`: not known) and `site`. App ids
/// past 24 bits count as unknown.
pub fn pack(app: Option<AppId>, site: Site) -> Tag {
    let app = app.filter(|&a| a < APP_MASK).unwrap_or(APP_MASK);
    let site = match site {
        Site::None => 0,
        Site::Unidentified => UNIDENTIFIED,
        Site::Slot(k) if usize::from(k) < MAX_SITES => u32::from(k) + 1,
        Site::Slot(_) => 0,
    };
    if app == APP_MASK && site == 0 {
        return NO_TAG;
    }
    site << 24 | app
}

/// The inverse of [`pack`].
pub fn unpack(tag: Tag) -> (Option<AppId>, Site) {
    if tag == NO_TAG {
        return (None, Site::None);
    }
    let app = tag & APP_MASK;
    let app = (app != APP_MASK).then_some(app);
    let site = match tag >> 24 {
        0 => Site::None,
        UNIDENTIFIED => Site::Unidentified,
        k @ 1..=253 => Site::Slot((k - 1) as u8),
        _ => Site::None,
    };
    (app, site)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let apps = [None, Some(0), Some(1), Some(77), Some(APP_MASK - 1)];
        let sites = [
            Site::None,
            Site::Unidentified,
            Site::Slot(0),
            Site::Slot(9),
            Site::Slot((MAX_SITES - 1) as u8),
        ];
        for app in apps {
            for site in sites {
                assert_eq!(unpack(pack(app, site)), (app, site), "{app:?} {site:?}");
            }
        }
    }

    #[test]
    fn without_sites_tags_are_app_ids() {
        assert_eq!(pack(Some(42), Site::None), 42);
        assert_eq!(pack(None, Site::None), NO_TAG);
        assert_eq!(unpack(NO_TAG), (None, Site::None));
    }

    #[test]
    fn out_of_range_parts_degrade_instead_of_colliding() {
        assert_eq!(
            unpack(pack(Some(APP_MASK), Site::Slot(3))),
            (None, Site::Slot(3))
        );
        assert_eq!(
            unpack(pack(Some(u32::MAX - 1), Site::None)),
            (None, Site::None)
        );
        assert_eq!(pack(Some(5), Site::Slot(MAX_SITES as u8)), 5);
        assert_ne!(pack(None, Site::Unidentified), NO_TAG);
    }
}
