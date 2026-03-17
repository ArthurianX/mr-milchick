use crate::tone::category::ToneCategory;

pub fn messages_for(category: ToneCategory) -> &'static [&'static str] {
    match category {
        ToneCategory::Observation => &[
            "Mr. Milchick is reviewing the situation.",
            "The request has been received and is being considered.",
            "Structural evaluation is currently in progress.",
            "A measured inspection has begun.",
        ],
        ToneCategory::Refinement => &[
            "A refinement opportunity has been identified.",
            "This request would benefit from additional alignment.",
            "Minor structural calibration is recommended.",
            "Further organization will improve the experience.",
        ],
        ToneCategory::Resolution => &[
            "The matter has been handled pleasantly.",
            "Structural harmony has been achieved.",
            "All required expectations are now satisfied.",
            "The request reflects commendable discipline.",
        ],
        ToneCategory::Blocking => &[
            "This merge request is not yet ready for a music dance experience.",
            "Progress is temporarily paused for structural reasons.",
            "Advancement requires additional refinement.",
            "This trajectory cannot presently continue.",
        ],
        ToneCategory::Praise => &[
            "This request demonstrates admirable clarity.",
            "The structure is unusually satisfying.",
            "Compliance has been achieved with elegance.",
            "This outcome is deeply pleasing.",
        ],
    }
}
