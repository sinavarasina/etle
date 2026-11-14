pub mod rows;

use relm4::{
    factory::{DynamicIndex, FactoryComponent, FactoryVecDeque},
    gtk::{self, prelude::*},
};

pub trait DiffRow: FactoryComponent<Index = DynamicIndex> {
    fn matches_data(&self, data: &Self::Init) -> bool;
    fn apply_data(&mut self, data: Self::Init);
}

pub fn sync_factory_rows<C: DiffRow>(factory: &mut FactoryVecDeque<C>, next: Vec<C::Init>) {
    let mut guard = factory.guard();
    while guard.len() > next.len() {
        guard.pop_back();
    }
    let existing_len = guard.len();
    for (idx, item) in next.into_iter().enumerate() {
        if idx < existing_len {
            let unchanged = guard.get(idx).is_some_and(|cur| cur.matches_data(&item));
            if !unchanged && let Some(cur) = guard.get_mut(idx) {
                cur.apply_data(item);
            }
        } else {
            guard.push_back(item);
        }
    }
}

pub fn restore_listbox_selection(list: &gtk::ListBox, target_idx: Option<usize>) {
    let current_idx = list
        .selected_row()
        .and_then(|row| usize::try_from(row.index()).ok());
    if current_idx == target_idx {
        return;
    }

    match target_idx {
        Some(idx) => {
            if let Ok(raw_idx) = i32::try_from(idx)
                && let Some(row) = list.row_at_index(raw_idx)
            {
                list.select_row(Some(&row));
            }
        }
        None => list.unselect_all(),
    }
}
