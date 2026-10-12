use serde_json::{Map, Value};

pub(crate) fn document_is_public(document: &Value) -> bool {
    let Some(limitation) = document.get("limitation").and_then(Value::as_object) else {
        return document.is_object();
    };
    if required(limitation, "auth_required")
        || required(limitation, "payment_required")
        || required(limitation, "restricted_writes")
    {
        return false;
    }
    accepts_note_kind(limitation)
}

fn required(limitation: &Map<String, Value>, name: &str) -> bool {
    limitation.get(name).and_then(Value::as_bool) == Some(true)
}

fn accepts_note_kind(limitation: &Map<String, Value>) -> bool {
    let Some(kinds) = limitation.get("kinds") else {
        return true;
    };
    kinds.as_array().is_some_and(|kinds| {
        kinds
            .iter()
            .any(|kind| kind.as_u64() == Some(u64::from(nonograph_nip44::KIND)))
    })
}

#[cfg(test)]
mod tests {
    use super::document_is_public;

    fn document(raw: &str) -> serde_json::Value {
        serde_json::from_str(raw).expect("nip11 fixture")
    }

    #[test]
    fn an_open_document_is_public() {
        let document = document(include_str!("../test/nip11/open.json"));
        assert!(document_is_public(&document));
    }

    #[test]
    fn auth_required_is_not_public() {
        let document = document(include_str!("../test/nip11/auth.json"));
        assert!(!document_is_public(&document));
    }

    #[test]
    fn payment_required_is_not_public() {
        let document = document(include_str!("../test/nip11/payment.json"));
        assert!(!document_is_public(&document));
    }

    #[test]
    fn restricted_writes_are_not_public() {
        let document = document(include_str!("../test/nip11/restricted.json"));
        assert!(!document_is_public(&document));
    }

    #[test]
    fn a_kind_list_that_contains_30323_is_public() {
        let document = document(include_str!("../test/nip11/kinds-with.json"));
        assert!(document_is_public(&document));
    }

    #[test]
    fn a_kind_list_without_30323_is_not_public() {
        let document = document(include_str!("../test/nip11/kinds-without.json"));
        assert!(!document_is_public(&document));
    }
}
