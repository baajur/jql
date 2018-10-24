extern crate regex;
extern crate serde_json;

use regex::Regex;
use serde_json::Value;
use types::{Selection, Selector};
use utils::display_node_or_range;

/// Get the trimmed text of the match with a default of an empty
/// string if the group didn't participate in the match.
fn get_selector(capture: &regex::Captures<'_>) -> Selector {
    let cap = capture.get(0).map_or("", |m| m.as_str()).trim();
    if cap.starts_with('\"') {
        // let cap_string = String::from(cap);
        // Drop the enclosing double quotes in this case.
        // let inner_cap = cap_string[1..cap_string.len() - 1];
        Selector::Default(String::from(&cap[1..cap.len() - 1]))
    } else {
        // Array range, e.g. 0:3.
        lazy_static! {
            static ref RANGE_REGEX: Regex = Regex::new(r"(\d+):(\d+)").unwrap();
        }
        let ranges: Vec<(&str, &str)> = RANGE_REGEX
            .captures_iter(cap)
            .map(|capture| {
                (
                    capture.get(1).map_or("", |m| m.as_str()),
                    capture.get(2).map_or("", |m| m.as_str()),
                )
            }).collect();
        if ranges.is_empty() {
            // Returns the initial captured value.
            Selector::Default(String::from(cap))
        } else {
            // Returns the range as a tuple of the form (start,end).
            let (start, end) = &ranges[0];
            Selector::Range((
                usize::from_str_radix(start, 10).unwrap(),
                usize::from_str_radix(end, 10).unwrap(),
            ))
        }
    }
}

pub fn array_walker(
    i: usize,
    index: isize,
    inner_json: &Value,
    s: &str,
    selector: &[Selector],
) -> Result<Value, String> {
    // A Negative index has been provided.
    if (index).is_negative() {
        return Err(String::from("Invalid negative array index"));
    }

    // A JSON null value has been found (array).
    if inner_json[index as usize] == Value::Null {
        let error_message = match inner_json.as_array() {
            // Trying to access an out of bound index on a node
            // or on the root element.
            Some(array) => {
                if selector.len() == 1 {
                    [
                        "Index (",
                        s,
                        ") is out of bound, root element has a length of",
                        &(array.len()).to_string(),
                    ]
                        .join(" ")
                } else {
                    [
                        "Index (",
                        s,
                        ") is out of bound,",
                        &display_node_or_range(&selector[i - 1], false),
                        "has a length of",
                        &(array.len()).to_string(),
                    ]
                        .join(" ")
                }
            }
            // Trying to access an index on a node which is not
            // an array.
            None => {
                if selector.len() == 1 {
                    ["Root element is not an array"].join(" ")
                } else {
                    [
                        &display_node_or_range(&selector[i - 1], true),
                        "is not an array",
                    ]
                        .join(" ")
                }
            }
        };

        return Err(error_message);
    }

    // Match found.
    Ok(inner_json[index as usize].clone())
}

pub fn range_selector(
    i: usize,
    inner_json: &Value,
    start: usize,
    end: usize,
    selector: &[Selector],
) -> Result<Value, String> {
    let is_default = start < end;

    // Check the range validity.
    if inner_json.as_array().unwrap().len() < start
        || inner_json.as_array().unwrap().len() < (end + 1)
    {
        return Err(if selector.len() == 1 {
            [
                "Range (",
                start.to_string().as_str(),
                ":",
                end.to_string().as_str(),
                ") is out of bound, root element has a length of",
                &(inner_json.as_array().unwrap().len()).to_string(),
            ]
                .join(" ")
        } else {
            [
                "Range (",
                start.to_string().as_str(),
                ":",
                end.to_string().as_str(),
                ") is out of bound,",
                &display_node_or_range(&selector[i - 1], false),
                "has a length of",
                &(inner_json.as_array().unwrap().len()).to_string(),
            ]
                .join(" ")
        });
    }

    Ok(if is_default {
        json!(inner_json.as_array().unwrap()[start..=end])
    } else {
        // Get the normalized slice selection, i.e. from end to start.
        let normalized_range_selection =
            json!(inner_json.as_array().unwrap()[end..=start]);
        // Reverse it.
        let reversed_range_selection: Vec<&Value> = normalized_range_selection
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .collect();
        json!(reversed_range_selection)
    })
}

fn group_walker(capture: &regex::Captures<'_>, json: &Value) -> Selection {
    lazy_static! {
        static ref SUB_GROUP_REGEX: Regex =
            Regex::new(r#"("[^"]+")|([^.]+)"#).unwrap();
    }
    let mut inner_json = json.clone();
    let group = capture.get(0).map_or("", |m| m.as_str());
    // Capture sub-groups of double quoted selectors and simple ones surrounded
    // by dots.
    let selector: Vec<Selector> = SUB_GROUP_REGEX
        .captures_iter(group)
        .map(|capture| get_selector(&capture))
        .collect();

    // Returns a Result of values or an Err early on, stopping the iteration
    // as soon as the latter is encountered.
    let items: Selection = selector
        .iter()
        .enumerate()
        .map(|(i, s)| -> Result<Value, String> {
            match s {
                // Default selector.
                Selector::Default(s) => {
                    // Array case.
                    if let Ok(index) = s.parse::<isize>() {
                        return match array_walker(
                            i,
                            index,
                            &inner_json.clone(),
                            s,
                            &selector,
                        ) {
                            Ok(json) => {
                                inner_json = json.clone();
                                Ok(json.clone())
                            }
                            Err(error) => Err(error),
                        };
                    }

                    // A JSON null value has been found (non array).
                    if inner_json[s] == Value::Null {
                        if i == 0 {
                            Err(["Node (", s, ") is not the root element"]
                                .join(" "))
                        } else {
                            Err([
                                "Node (",
                                s,
                                ") not found on parent",
                                &display_node_or_range(&selector[i - 1], false),
                            ]
                                .join(" "))
                        }
                    } else {
                        inner_json = inner_json[s].clone();
                        Ok(inner_json.clone())
                    }
                }
                // Range selector.
                Selector::Range((start, end)) => match range_selector(
                    i,
                    &inner_json.clone(),
                    *start,
                    *end,
                    &selector,
                ) {
                    Ok(json) => {
                        inner_json = json.clone();
                        Ok(json.clone())
                    }
                    Err(error) => Err(error),
                },
            }
        }).collect();

    // Check for empty selection, in this case we assume that the user expects
    // to get back the complete raw JSON back for this group.
    match items {
        Ok(items) => {
            if items.is_empty() {
                Ok(vec![json.clone()])
            } else {
                Ok(items)
            }
        }
        Err(items) => Err(items),
    }
}

/// Given some selector walk over the JSON file.
pub fn walker(json: &Value, selector: Option<&str>) -> Result<Value, String> {
    // A Selector has been found.
    if let Some(selector) = selector {
        lazy_static! {
            static ref GROUP_REGEX: Regex = Regex::new(r"([^,]+)").unwrap();
        }
        // Capture groups separated by commas, apply the selector for the
        // current group and return a Result of values or an Err early on.
        let groups: Result<Vec<Value>, String> = GROUP_REGEX
            .captures_iter(selector)
            .map(|capture| group_walker(&capture, json))
            .map(|s| -> Result<Value, String> {
                match s {
                    Ok(items) => Ok(items.last().unwrap().clone()),
                    Err(error) => Err(error.clone()),
                }
            }).collect();

        return match groups {
            Ok(groups) => match groups.len() {
                0 => Err(String::from("Empty selection")),
                // One group.
                1 => Ok(json!(groups[0])),
                // Multiple groups.
                _ => Ok(json!(groups)),
            },
            Err(error) => Err(error),
        };
    }
    // Nothing found.
    Err(String::from("No selector found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The following constants are all valid JSON.
    // https://tools.ietf.org/html/rfc8259#section-13

    const SINGLE_VALUE_DATA: &str = r#"1337"#;

    const ARRAY_DATA: &str = r#"[1, 2, 3]"#;

    const DATA: &str = r#"{
        "array": [1, 2, 3],
        "nested": {
            "a": "one",
            "b": "two",
            "c": "three"
        },
        "number": 1337,
        "text": "some text",
        ".property..": "This is valid JSON!",
        "\"": "This is valid JSON as well",
        "mix": [{ "first": 1 }],
        "range": [1, 2, 3, 4, 5, 6, 7]
    }"#;

    #[test]
    fn get_text() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("text");
        assert_eq!(Ok(json["text"].clone()), walker(&json, selector));
    }

    #[test]
    fn get_number() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("number");
        assert_eq!(Ok(json["number"].clone()), walker(&json, selector));
    }

    #[test]
    fn get_array() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("array");
        assert_eq!(Ok(json["array"].clone()), walker(&json, selector));
    }

    #[test]
    fn get_item_in_array() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("array.0");
        assert_eq!(Ok(json["array"][0].clone()), walker(&json, selector));
    }

    #[test]
    fn get_out_of_bound_item_in_array() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("array.3");
        assert_eq!(
            Err(String::from(
                "Index ( 3 ) is out of bound, node ( array ) has a length of 3"
            )),
            walker(&json, selector)
        );
    }

    #[test]
    fn get_out_of_bound_item_in_root_array() {
        let json_array: Value = serde_json::from_str(ARRAY_DATA).unwrap();
        let array_selector: Option<&str> = Some("3");
        assert_eq!(
            Err(String::from(
                "Index ( 3 ) is out of bound, root element has a length of 3"
            )),
            walker(&json_array, array_selector)
        );
    }

    #[test]
    fn get_negative_index_in_array() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("array.-1");
        assert_eq!(
            Err(String::from("Invalid negative array index")),
            walker(&json, selector)
        );
    }

    #[test]
    fn get_negative_index_in_root_array() {
        let json_array: Value = serde_json::from_str(ARRAY_DATA).unwrap();
        let array_selector: Option<&str> = Some("-1");
        assert_eq!(
            Err(String::from("Invalid negative array index")),
            walker(&json_array, array_selector)
        );
    }

    #[test]
    fn get_index_in_non_array() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("text.1");
        let root_selector: Option<&str> = Some("1");
        assert_eq!(
            Err(String::from("Node ( text ) is not an array")),
            walker(&json, selector)
        );
        assert_eq!(
            Err(String::from("Root element is not an array")),
            walker(&json, root_selector)
        );
    }

    #[test]
    fn get_non_existing_root_node() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("foo");
        assert_eq!(
            Err(String::from("Node ( foo ) is not the root element")),
            walker(&json, selector)
        );
    }

    #[test]
    fn get_non_existing_child_node() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("nested.d");
        assert_eq!(
            Err(String::from(
                "Node ( d ) not found on parent node ( nested )"
            )),
            walker(&json, selector)
        );
    }

    #[test]
    fn get_existing_child_node() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("nested.a");
        assert_eq!(Ok(json["nested"]["a"].clone()), walker(&json, selector));
    }

    #[test]
    fn get_single_value() {
        let json_single_value: Value =
            serde_json::from_str(SINGLE_VALUE_DATA).unwrap();
        let selector: Option<&str> = Some(".");
        assert_eq!(
            Ok(json_single_value.clone()),
            walker(&json_single_value, selector)
        );
    }

    #[test]
    fn get_empty() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("");
        assert_eq!(
            Err(String::from("Empty selection")),
            walker(&json, selector)
        );
    }

    #[test]
    fn get_raw_json() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some(".");
        assert_eq!(Ok(json.clone()), walker(&json, selector));
    }

    #[test]
    fn get_weird_json() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let dot_selector: Option<&str> = Some(r#"".property..""#);
        let quote_selector: Option<&str> = Some(r#"""""#);
        assert_eq!(
            Ok(json[".property.."].clone()),
            walker(&json, dot_selector)
        );
        assert_eq!(Ok(json[r#"""#].clone()), walker(&json, quote_selector));
    }

    #[test]
    fn get_mix_json() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let mix_selector: Option<&str> = Some("mix.0.first");
        assert_eq!(
            Ok(json["mix"][0]["first"].clone()),
            walker(&json, mix_selector)
        );
    }

    #[test]
    fn get_range() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("range.2:5");
        assert_eq!(Ok(json!([3, 4, 5, 6])), walker(&json, selector));
    }

    #[test]
    fn get_one_item_range() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("range.2:2");
        assert_eq!(Ok(json!([3])), walker(&json, selector));
    }

    #[test]
    fn get_reversed_range() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("range.5:2");
        assert_eq!(Ok(json!([6, 5, 4, 3])), walker(&json, selector));
    }

    #[test]
    fn get_original_from_reversed_range() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("range.5:2.3:0");
        assert_eq!(Ok(json!([3, 4, 5, 6])), walker(&json, selector));
    }

    #[test]
    fn get_out_of_bound_range() {
        let json: Value = serde_json::from_str(DATA).unwrap();
        let selector: Option<&str> = Some("range.6:7");
        assert_eq!(
            Err(String::from("Range ( 6 : 7 ) is out of bound, node ( range ) has a length of 7")),
            walker(&json, selector)
        );
    }
}
