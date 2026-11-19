use crate::saved_pet::SavedPet;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Column {
    Name,
    Species,
    Id,
}

#[derive(Clone, Copy)]
pub(crate) struct Sort {
    field: Column,
    descending: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Self {
            field: Column::Name,
            descending: false,
        }
    }
}

impl Sort {
    pub fn toggle(&mut self, field: Column) {
        if self.field == field {
            self.descending = !self.descending;
        } else {
            self.field = field;
            self.descending = false;
        }
    }

    pub fn aria(&self, field: Column) -> &'static str {
        if self.field != field {
            "none"
        } else if self.descending {
            "descending"
        } else {
            "ascending"
        }
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct Page {
    pub rows: Vec<SavedPet>,
    pub filtered: usize,
    pub page: usize,
    pub pages: usize,
    pub start: usize,
    pub end: usize,
}

pub(crate) fn project(
    pets: &[SavedPet],
    query: &str,
    sort: Sort,
    page: usize,
    size: usize,
) -> Page {
    let needle = query.trim().to_lowercase();
    let mut rows = pets
        .iter()
        .filter(|pet| {
            [&pet.name, &pet.species, &pet.id]
                .iter()
                .any(|value| value.to_lowercase().contains(&needle))
        })
        .cloned()
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        let field = |p: &SavedPet| match sort.field {
            Column::Name => p.name.to_lowercase(),
            Column::Species => p.species.to_lowercase(),
            Column::Id => p.id.to_lowercase(),
        };
        let order = field(a).cmp(&field(b));
        let order = if sort.descending {
            order.reverse()
        } else {
            order
        };
        order.then_with(|| a.id.cmp(&b.id))
    });
    let filtered = rows.len();
    let size = if [10, 25, 50].contains(&size) {
        size
    } else {
        10
    };
    let pages = filtered.div_ceil(size).max(1);
    let page = page.clamp(1, pages);
    let offset = (page - 1) * size;
    let end = (offset + size).min(filtered);
    let start = if filtered == 0 { 0 } else { offset + 1 };
    Page {
        rows: rows.into_iter().skip(offset).take(size).collect(),
        filtered,
        page,
        pages,
        start,
        end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pet(id: &str, name: &str) -> SavedPet {
        SavedPet {
            id: id.into(),
            name: name.into(),
            species: "tortoise".into(),
        }
    }
    #[test]
    fn fuzzy_search_uses_names_species_and_ids_without_mutating_source() {
        let pets = vec![pet("A-1", "龜龜 Alice"), pet("b-2", "Bob")];
        for (q, n) in [
            (" alice ", 1),
            ("龜", 1),
            ("a-1", 1),
            ("TORToise", 2),
            ("%", 0),
        ] {
            assert_eq!(project(&pets, q, Sort::default(), 1, 10).filtered, n);
        }
        assert_eq!(pets[0].name, "龜龜 Alice");
    }
    #[test]
    fn sorting_is_stable_and_toggle_changes_direction() {
        let pets = vec![pet("b", "Same"), pet("a", "same"), pet("c", "Zoo")];
        let mut sort = Sort::default();
        let p = project(&pets, "", sort, 1, 10);
        assert_eq!(
            p.rows.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        sort.toggle(Column::Name);
        assert_eq!(sort.aria(Column::Name), "descending");
        assert_eq!(project(&pets, "", sort, 1, 10).rows[0].id, "c");
    }
    #[test]
    fn paging_clamps_after_filter_or_refresh_and_handles_empty() {
        let pets = (0..27)
            .map(|i| pet(&format!("id{i:02}"), &format!("寵物{i:02}")))
            .collect::<Vec<_>>();
        let p = project(&pets, "", Sort::default(), 99, 10);
        assert_eq!(
            (p.page, p.pages, p.start, p.end, p.rows.len()),
            (3, 3, 21, 27, 7)
        );
        let p = project(&pets, "寵物00", Sort::default(), 3, 10);
        assert_eq!((p.page, p.pages, p.rows.len()), (1, 1, 1));
        let p = project(&[], "", Sort::default(), 9, 0);
        assert_eq!((p.page, p.pages, p.start, p.end), (1, 1, 0, 0));
    }
}
