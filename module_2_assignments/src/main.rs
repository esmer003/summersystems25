//assignment 1
fn sum_with_step(total: &mut i32, low: i32, high: i32, step: i32) {
    let mut current = low;
    while current <= high {
        *total += current;
        current += step;
    }
}

//assignment 2
fn most_frequent_word(text: &str) -> (String, usize) {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut counts: Vec<(String, usize)> = Vec::new();

    for &word in &words {
        let mut found = false;

        for (existing_word, count)in &mut counts {
            if existing_word == word {
                *count += 1;
                found = true;
                break;
            }
        }
        if !found {
            counts.push((word.to_string(), 1));
        }
    }
    let mut max_word = String::new();
    let mut max_count = 0;
    
    for (word, count) in &counts {
        if *count > max_count {
            max_count = *count;
            max_word = word.clone();
        }
    }
    (max_word, max_count)
}
fn main() {
    
    //assignment 1
    let mut result = 0;
    sum_with_step(&mut result, 0, 100, 1);
    println!("Sum 0 to 100, step 1: {}", result);

    result = 0;
    sum_with_step(&mut result, 0, 10, 2);
    println!("Sum 0 to 10, step 2: {}", result);

    result = 0;
    sum_with_step(&mut result, 5, 15, 3);
    println!("Sum 5 to 15, step 3: {}", result);

    //assignment 2
    let text = "the quick brown fox jumps over the lazy dog the quick brown fox";
    let (word, count) = most_frequent_word(text);
    println!("Most frequent word: \"{}\" ({} times)", word, count);

}
