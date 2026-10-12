use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NameID(pub u32);

#[derive(Debug, Default)]
pub struct NameInterner {
    ids: HashMap<Box<str>, NameID>,
}

impl NameInterner {

    pub fn intern(&mut self, name: &str) -> NameID {
        if let Some(&id) = self.ids.get(name) {
            return id;
        }

        let raw = u32::try_from(self.ids.len()).expect("ICE: identifier interner exhausted u32 IDs");

        let id = NameID(raw);
        self.ids.insert(name.into(), id);

        id
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}
