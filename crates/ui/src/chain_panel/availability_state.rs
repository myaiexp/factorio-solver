// The availability gate's state on `ChainPanel`: what it allows, adopting or
// reloading a save into it, and switching its mode.
use factorio_solver::availability::{all_available_recipe_names, Availability};

use super::{AvailabilityMode, ChainPanel};

impl ChainPanel {
    /// The `Availability` the current state describes: unrestricted in
    /// `Everything` mode, the edited set in `OnlyAvailable`.
    ///
    /// `pub(super)`: the controls and the tests both read it, and all of them
    /// live under `chain_panel`.
    pub(super) fn availability(&self) -> Availability {
        match self.availability_mode {
            AvailabilityMode::Everything => Availability::Everything,
            // `unwrap_or_default` reads an unseeded `None` as an empty set —
            // reachable only if `availability_mode` were ever set without
            // going through `set_availability_mode` below, which never
            // happens today. Treat it as the conservative answer (allow
            // only what `enabled: true` already grants for free) rather than
            // a case to paper over with a fallback to `Everything`.
            AvailabilityMode::OnlyAvailable => {
                Availability::Unlocked(self.available_recipes.clone().unwrap_or_default())
            }
        }
    }

    /// Decode a save and adopt its unlocked-recipe set wholesale, switching
    /// to `OnlyAvailable` so the import takes effect immediately.
    ///
    /// An import and a hand-edit fill the *same* field — that is the whole
    /// point of keying the gate on a recipe-name set rather than on a source.
    /// So a save replaces the set outright and the tick list below then edits
    /// it, which makes manual work corrections on top of an import rather than
    /// a competing second opinion the solver would have to arbitrate.
    ///
    /// A failed read changes nothing but the picker's status line: leaving the
    /// previous set in place beats replacing it with an empty one that would
    /// make every recipe look locked.
    pub(super) fn adopt_save(&mut self, path: &std::path::Path) {
        if let Some(set) = self.save_picker.select(path) {
            self.available_recipes = Some(set);
            self.availability_mode = AvailabilityMode::OnlyAvailable;
        }
    }

    /// Take a changed save back off disk, when doing so discards nothing.
    ///
    /// Deliberately does *not* touch `availability_mode`, which is the one
    /// thing that separates this from `adopt_save`. An import is a user
    /// action and may switch the panel into `OnlyAvailable`; a background
    /// refresh must not, or a user who deliberately switched back to
    /// `Everything` would find the gate re-applied by an autosave.
    pub(super) fn poll_save_reload(&mut self) {
        if let Some(set) = self.save_picker.poll(self.available_recipes.as_ref()) {
            self.available_recipes = Some(set);
        }
    }

    /// Switch modes, seeding the set from [`all_available_recipe_names`] the
    /// first time `OnlyAvailable` is entered — so the switch alone changes no
    /// solve result and the user's first edit is a removal. Seeding is keyed
    /// on `available_recipes.is_none()`, not on the mode being entered again,
    /// so toggling back to `Everything` and returning preserves whatever the
    /// user already unticked instead of re-seeding over it.
    ///
    /// `pub(crate)`, not private: `persist.rs::PanelState::apply` reuses this
    /// exact method to re-seed a restored `OnlyAvailable` mode that carries no
    /// set, rather than re-implementing the seeding rule a second time.
    pub(crate) fn set_availability_mode(&mut self, mode: AvailabilityMode) {
        if mode == AvailabilityMode::OnlyAvailable && self.available_recipes.is_none() {
            self.available_recipes = Some(all_available_recipe_names());
        }
        self.availability_mode = mode;
    }
}
