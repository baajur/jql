use apply_filter::apply_filter;
use flatten_json_array::flatten_json_array;
use get_selection::get_selection;
use serde_json::json;
use serde_json::Value;
use types::{Group, MaybeArray};

/// Walks through a group.
pub fn group_walker(
    (spread, selectors, filters): &Group,
    json: &Value,
) -> Result<Value, String> {
    // Empty group, return early.
    if selectors.is_empty() {
        return Err(String::from("Empty group"));
    }

    match get_selection(&selectors, &json) {
        Ok(ref items) => {
            // Check for an empty selection, in this case we assume that the user
            // expects to get back the complete raw JSON for this group.
            let output_json = if items.is_empty() {
                json.clone()
            } else {
                json!(items.last()).clone()
            };

            let is_spreading = spread.is_some();

            match apply_filter(&output_json, &filters) {
                Ok(filtered) => match filtered {
                    MaybeArray::Array(array) => Ok(if is_spreading {
                        flatten_json_array(&json!(array))
                    } else {
                        json!(array)
                    }),
                    MaybeArray::NonArray(single_value) => {
                        if is_spreading {
                            Err(String::from("Only arrays can be flattened"))
                        } else {
                            // We know that we are holding a single value
                            // wrapped inside a MaybeArray::NonArray enum.
                            // We need to pick the first item of the vector.
                            Ok(json!(single_value[0]))
                        }
                    }
                },
                Err(error) => Err(error),
            }
        }
        Err(items) => Err(items),
    }
}
