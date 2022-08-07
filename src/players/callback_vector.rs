use std::{collections::HashMap, fmt, ops::Deref, rc::Rc};

use crate::error::Error;

use super::PlayerState;

pub(crate) type Callback = Rc<dyn Fn(&PlayerState, bool) -> Result<(), Error>>;

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub(crate) enum CallbackKind {
    GovernmentOverviewGraph,
    ProbabilityTree
}

pub(crate) struct CallBackVec<T> {
    data : Vec<T>,
    callbacks : HashMap<CallbackKind, Callback>
}

impl<T> Deref for CallBackVec<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target { &self.data }
}

impl<T> Default for CallBackVec<T> {
    fn default() -> Self {
        Self {
            data : Default::default(),
            callbacks : HashMap::new()
        }
    }
}

impl<T : fmt::Debug> fmt::Debug for CallBackVec<T> {
    fn fmt(&self, f : &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallBackVec")
            .field("data", &self.data)
            .finish()
    }
}

impl<T> CallBackVec<T> {
    fn generate_callbacks(&self) -> Callback {
        let cloned_callbacks = self.callbacks.clone();
        Rc::new(move |ps, auto| {
            cloned_callbacks
                .iter()
                .map(|(_, cb)| cb(ps, auto))
                .collect()
        })
    }

    #[must_use]
    pub(crate) fn push(&mut self, item : T) -> Callback {
        self.data.push(item);

        self.generate_callbacks()
    }

    #[must_use]
    pub(crate) fn remove(&mut self, index : usize) -> Option<Callback> {
        if index < self.data.len() {
            self.data.remove(index);
            Some(self.generate_callbacks())
        }
        else {
            None
        }
    }

    pub(crate) fn register_callback(
        &mut self,
        kind : CallbackKind,
        callback : Callback
    ) -> Option<Callback> {
        self.callbacks.insert(kind, callback)
    }

    pub(crate) fn callback(&self) -> Callback { self.generate_callbacks() }
}
