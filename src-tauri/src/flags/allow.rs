//! The keys this client will actually take from a local file.
//!
//! Roblox turned the Fast Flag file into an allowlisted surface in late 2025.
//! The client still reads `ClientAppSettings.json`, and then discards every key
//! that is not on the published list without saying a word. A launcher that
//! offers 25,585 keys and calls all of them flags is therefore lying by
//! omission, so the list lives here and every screen that shows a key says which
//! side of it the key is on.
//!
//! Source: Roblox's own announcement, "Allowlist for local client configuration
//! via Fast Flags" — <https://devforum.roblox.com/t/3966569>, read 2026-11-18.
//! The list is theirs to change at any time; this is the copy the app ships, and
//! the copy in `catalog` is theirs to change too, which is why the screens say
//! "Roblox honours" rather than promising an effect.
//!
//! Two measured details sit around this list and matter as much as it does:
//!
//! - Studio is not covered by the allowlist at all. This app only writes the
//!   player's file, so that exemption is not offered anywhere.
//! - The value has to be a JSON *string*. `"FFlagDebugGraphicsPreferVulkan":
//!   "True"` loads Vulkan; the same key as `true` leaves the client on D3D11.
//!
//! Checked end to end on 0.748.0.7481526: with `"FFlagDebugGraphicsPreferVulkan":
//! "True"` in `<version>\ClientSettings\ClientAppSettings.json`, the next client
//! has `vulkan-1.dll` in its module list. Remove the file and it is back on
//! `d3d11.dll` with no Vulkan module at all.

/// Roblox's published allowlist, by their own grouping.
pub const KEYS: [&str; 18] = [
    // Geometry
    "DFIntCSGLevelOfDetailSwitchingDistance",
    "DFIntCSGLevelOfDetailSwitchingDistanceL12",
    "DFIntCSGLevelOfDetailSwitchingDistanceL23",
    "DFIntCSGLevelOfDetailSwitchingDistanceL34",
    // Rendering
    "FFlagHandleAltEnterFullscreenManually",
    "DFFlagTextureQualityOverrideEnabled",
    "DFIntTextureQualityOverride",
    "FIntDebugForceMSAASamples",
    "DFFlagDisableDPIScale",
    "FFlagDebugGraphicsPreferD3D11",
    "FFlagDebugSkyGray",
    "DFFlagDebugPauseVoxelizer",
    "DFIntDebugFRMQualityLevelOverride",
    "FIntFRMMaxGrassDistance",
    "FIntFRMMinGrassDistance",
    "FFlagDebugGraphicsPreferVulkan",
    "FFlagDebugGraphicsPreferOpenGL",
    // User interface
    "FIntGrassMovementReducedMotionFactor",
];

/// Where the list came from, shown beside it so a person can check the source
/// rather than trust a build from last week.
pub const SOURCE: &str = "https://devforum.roblox.com/t/3966569";

/// Whether the client will take this key from a local file.
///
/// Case-insensitive because key names are typed by hand in the field, and a
/// lowercased copy of a real key would otherwise be reported as ignored when the
/// client itself matches names case-insensitively.
pub fn honours(key: &str) -> bool {
    KEYS.iter().any(|allowed| allowed.eq_ignore_ascii_case(key))
}

/// How many of these keys the client will honour.
pub fn count<'a>(keys: impl IntoIterator<Item = &'a str>) -> usize {
    keys.into_iter().filter(|key| honours(key)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_what_roblox_published() {
        assert_eq!(KEYS.len(), 18);
        assert!(honours("FFlagDebugGraphicsPreferVulkan"));
        assert!(honours("FIntGrassMovementReducedMotionFactor"));
    }

    #[test]
    fn the_flag_everybody_wants_is_not_on_it() {
        // The most requested key in the community, and measured dead: it is not
        // on the allowlist, so the client writes it into the file and ignores it.
        assert!(!honours("DFIntTaskSchedulerTargetFps"));
        assert!(!honours("FFlagDebugDisplayFPS"));
    }

    #[test]
    fn names_match_regardless_of_case() {
        assert!(honours("fflagdebugskygray"));
        assert!(honours("FFLAGDEBUGSKYGRAY"));
    }

    #[test]
    fn counting_reads_every_key() {
        let keys = [
            "FFlagDebugSkyGray",
            "DFIntTaskSchedulerTargetFps",
            "DFIntCSGLevelOfDetailSwitchingDistance",
        ];
        assert_eq!(count(keys), 2);
    }
}
