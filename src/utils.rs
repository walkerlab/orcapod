pub fn get_struct_name<T>(_: T) -> String {
    std::any::type_name::<T>()
        .split("::")
        .collect::<Vec<&str>>()
        .get(2)
        .unwrap()
        .to_string()
}
