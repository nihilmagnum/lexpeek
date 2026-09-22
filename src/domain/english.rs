/// Возможные начальные формы английского слова, от более вероятной к менее.
pub fn base_forms(word: &str) -> Vec<String> {
    let mut forms = Vec::new();
    if let Some(stem) = word.strip_suffix("'s") {
        forms.push(stem.to_owned());
    }
    if let Some(stem) = word.strip_suffix("ies") {
        forms.push(format!("{stem}y"));
    }
    if let Some(stem) = word.strip_suffix("ves") {
        forms.push(format!("{stem}f"));
        forms.push(format!("{stem}fe"));
    }
    if let Some(stem) = word.strip_suffix("es") {
        forms.push(stem.to_owned());
    }
    if let Some(stem) = word.strip_suffix('s')
        && !stem.ends_with('s')
    {
        forms.push(stem.to_owned());
    }
    if let Some(stem) = word.strip_suffix("ied") {
        forms.push(format!("{stem}y"));
    }
    if let Some(stem) = word.strip_suffix("ying") {
        forms.push(format!("{stem}ie"));
    }
    for suffix in ["ed", "ing", "er", "est"] {
        if let Some(stem) = word.strip_suffix(suffix) {
            forms.extend(verb_stems(stem));
        }
    }
    for suffix in ["ier", "iest"] {
        if let Some(stem) = word.strip_suffix(suffix) {
            forms.push(format!("{stem}y"));
        }
    }

    let mut unique = Vec::new();
    for form in forms {
        if form.chars().count() >= 2 && form != word && !unique.contains(&form) {
            unique.push(form);
        }
    }
    unique
}

/// Основа после отбрасывания -ed/-ing/-er/-est: как есть, с немой e и без удвоенной согласной.
fn verb_stems(stem: &str) -> Vec<String> {
    let mut stems = vec![stem.to_owned(), format!("{stem}e")];
    let mut chars = stem.chars().rev();
    if let (Some(last), Some(previous)) = (chars.next(), chars.next())
        && last == previous
        && !"aeiouls".contains(last)
    {
        stems.push(stem[..stem.len() - last.len_utf8()].to_owned());
    }
    stems
}

#[cfg(test)]
mod tests {
    use super::base_forms;

    #[test]
    fn finds_base_form_among_candidates() {
        let cases = [
            ("cities", "city"),
            ("wolves", "wolf"),
            ("boxes", "box"),
            ("games", "game"),
            ("carried", "carry"),
            ("dying", "die"),
            ("running", "run"),
            ("making", "make"),
            ("played", "play"),
            ("bigger", "big"),
            ("happiest", "happy"),
            ("player's", "player"),
        ];
        for (word, base) in cases {
            assert!(base_forms(word).contains(&base.to_owned()), "{word} -> {base}");
        }
    }
}
