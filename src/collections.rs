use indexmap::IndexMap;

pub trait GetKey {
    fn get_key(&self) -> &str;
}

#[allow(dead_code)]
pub(crate) trait IntoKeyedIndexMap<S> {
    fn into_keyed_index_map(self) -> IndexMap<String, S>;
}

impl<T: GetKey> IntoKeyedIndexMap<T> for Vec<T> {
    fn into_keyed_index_map(self) -> IndexMap<String, T> {
        self.into_iter()
            .map(|x| (x.get_key().to_string(), x))
            .collect()
    }
}
