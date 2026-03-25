use crate::core::tone::category::ToneCategory;

pub fn messages_for(category: ToneCategory) -> &'static [&'static str] {
    match category {
        ToneCategory::Observation => &[
            "Mr. Milchick is reviewing the situation.",
            "The request has been received and is being considered.",
            "The request has entered a pleasant phase of inspection.",
            "Structural evaluation is currently in progress.",
            "A measured inspection has begun.",
            "Mr. Milchick has begun a careful review of this request.",
            "The department is considering the present structure with interest.",
        ],
        ToneCategory::Refinement => &[
            "A refinement opportunity has been identified.",
            "This request would benefit from additional alignment.",
            "Minor structural calibration is recommended.",
            "Further organization will improve the experience.",
            "A modest refinement would improve departmental harmony.",
            "This request is close, though not yet pleasingly complete.",
        ],
        ToneCategory::Resolution => &[
            "The matter has been handled pleasantly.",
            "Structural harmony has been achieved.",
            "All required expectations are now satisfied.",
            "The request reflects commendable discipline.",
            "This matter has concluded in an orderly fashion. :)",
            "The department considers this outcome satisfactory.",
        ],
        ToneCategory::Blocking => &[
            "This merge request is not yet ready for a music dance experience.",
            "Progress is temporarily paused for structural reasons.",
            "Advancement requires additional refinement.",
            "This trajectory cannot presently continue.",
            "Progress is paused pending a more respectful structure. :|",
            "Advancement will resume after the necessary refinements occur.",
        ],
        ToneCategory::Praise => &[
            "This request demonstrates admirable clarity.",
            "The structure is unusually satisfying.",
            "Compliance has been achieved with elegance.",
            "This outcome is deeply pleasing.",
            "This level of clarity has been noted with appreciation.",
            "The department appreciates this degree of discipline. :)",
        ],
        ToneCategory::ReviewRequest => &[
            "The department would appreciate a timely review.",
            "A pleasant review opportunity has arrived for your consideration.",
            "Attention is requested for a newly aligned merge request.",
            "A structured look at this merge request would be warmly received.",
            "Your review would be both timely and pleasing.",
            "A newly aligned merge request awaits your considerate attention.",
        ],
        ToneCategory::NoAction => &[
            "No further action is required at this time.",
            "The present structure requires no additional intervention.",
            "No findings were produced, which the department appreciates.",
            "Everything appears orderly enough to proceed without adjustment.",
        ],
        ToneCategory::ReviewerAssigned => &[
            "Appropriate reviewers have been invited into the experience.",
            "Reviewer alignment has been completed pleasantly.",
            "The department has identified suitable reviewers for this request.",
            "Reviewer participation has been arranged in an orderly fashion.",
        ],
    }
}
