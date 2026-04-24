pub(crate) fn fit_to_length(input: String, max_length: usize) -> String {
    if char_len(&input) <= max_length {
        input
    } else {
        elide_url(&input, max_length)
    }
}

// if url too long
fn elide_url(input: &str, max_length: usize) -> String {
    if max_length <= 1 {
        return "…".to_owned();
    }

    let (base, tail) = split_tail(input);
    let tail = if tail.is_empty() {
        ""
    } else if tail.starts_with('?') {
        "?…"
    } else {
        "#…"
    };

    let base_budget = max_length.saturating_sub(char_len(tail));
    if let Some(base) = elide_path(base, base_budget) {
        let result = base + tail;
        if char_len(&result) <= max_length {
            return result;
        }
    }

    truncate_end(input, max_length)
}

// split tailing query & fragment
fn split_tail(input: &str) -> (&str, &str) {
    let query = input.find('?');
    let fragment = input.find('#');
    let tail_start = match (query, fragment) {
        (Some(query), Some(fragment)) => query.min(fragment),
        (Some(query), None) => query,
        (None, Some(fragment)) => fragment,
        (None, None) => return (input, ""),
    };

    input.split_at(tail_start)
}

fn elide_path(input: &str, max_length: usize) -> Option<String> {
    if char_len(input) <= max_length {
        return Some(input.to_owned());
    }

    let path_start = input.find('/')?;
    let head = &input[..path_start];
    let path = &input[path_start + 1..];
    let trailing_slash = path.ends_with('/');
    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    if segments.len() < 2 {
        return None;
    }

    let mut best = None;
    let mut best_score = (0, 0, 0);

    for suffix_count in 1..segments.len() {
        for prefix_count in 0..(segments.len() - suffix_count) {
            let candidate =
                build_elided_path(head, &segments, prefix_count, suffix_count, trailing_slash);
            let len = char_len(&candidate);

            if len <= max_length {
                let score = (prefix_count, suffix_count, len);
                if score > best_score {
                    best_score = score;
                    best = Some(candidate);
                }
            }
        }
    }

    best
}

fn build_elided_path(
    head: &str,
    segments: &[&str],
    prefix_count: usize,
    suffix_count: usize,
    trailing_slash: bool,
) -> String {
    let mut result = String::with_capacity(head.len() + 8);
    result.push_str(head);
    result.push('/');

    for segment in &segments[..prefix_count] {
        result.push_str(segment);
        result.push('/');
    }

    result.push('…');

    for segment in &segments[segments.len() - suffix_count..] {
        result.push('/');
        result.push_str(segment);
    }

    if trailing_slash {
        result.push('/');
    }

    result
}

fn truncate_end(input: &str, max_length: usize) -> String {
    if max_length <= 1 {
        return "…".to_owned();
    }

    input.chars().take(max_length - 1).collect::<String>() + "…"
}

fn char_len(input: &str) -> usize {
    input.chars().count()
}
