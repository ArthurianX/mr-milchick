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
        ToneCategory::ReviewRequest => &[
            "The department would appreciate a timely review.",
            "A pleasant review opportunity has arrived for your consideration.",
            "Attention is requested for a newly aligned merge request.",
            "A structured look at this merge request would be warmly received.",
        ],
    }
}
