// declaration of constant
const FREEZING_POINT: f64 = 32.0;

// functions (F to C and vice versa)
fn fahrenheit_to_celsius(f: f64) -> f64 {
    (f - FREEZING_POINT) * 5.0/9.0
}
fn celsius_to_fahrenheit(c: f64) -> f64 {
    (c * 9.0/5.0) + FREEZING_POINT
}
fn is_even(n: i32) -> bool {
    n % 2 == 0 
}
fn check_guess(guess: i32, secret: i32) -> i32 {
    if guess == secret {
        0
    } else if guess > secret {
        1
    } else {
        -1
    }
}

//Assignment 1
fn run_assignment1() {
    // mutable variable
    let mut temp_f: f64 = 32.0;

    let temp_c = fahrenheit_to_celsius(temp_f);
    println!("{:.1}°F is {:.1}°C", temp_f, temp_c);

    // Loop to print conversions for the next 5 temperatures
    for _ in 0..5 {
        temp_f += 1.0; 
        let temp_c = fahrenheit_to_celsius(temp_f);
        println!("{:.1}°F is {:.1}°C", temp_f, temp_c);
    }
}

// Assignment 2
fn run_assignment2() {

    let numbers: [i32; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

    for &n in &numbers {
        if n % 15 == 0 {
            println!("FizzBuzz");
        } else if n % 3 == 0 {
            println!("Fizz");
        } else if n % 5 == 0 {
            println!("Buzz");
        } else {
            let parity = if is_even(n) { "Even" } else { "Odd" };
            println!("{parity}");
        }
    }
    let mut i = 0;
    let mut sum = 0;
    while i < numbers.len() {
        sum += numbers[i];
        i += 1;
    }
    println!("Sum of all numbers: {sum}");

    // to find the largest number
    let mut largest = numbers[0];
    let mut j = 1;
    loop {
        if j >= numbers.len() {
            break;
        }
        if numbers[j] > largest {
            largest = numbers[j];
        }
        j += 1;
    }
    println!("Largest number: {largest}");
}

// Assignment 3
fn run_assignment3() {
    
    let mut secret: i32 = 42;
    let mut guess: i32 = 10;
    let mut attempts = 0;

    loop {
        attempts += 1;

        let result = check_guess(guess, secret);

        if result == 0 {
            println!("{guess} -> Correct!");
            break;
        } else if result == 1 {
            println!("{guess} -> Too high");
            guess -= 1;
        } else {
            println!("{guess} -> Too low");
            guess += 1;
        }
    }

    println!("It took {attempts} guesses to find the secret ({secret}).");
}


fn main() {
    run_assignment1();

    run_assignment2();

    run_assignment3();
}