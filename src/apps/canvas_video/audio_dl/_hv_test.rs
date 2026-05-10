fn main() {
    let r = reqwest::header::HeaderValue::from_str("https://例.cn");
    println!("result: {:?}", r);
    let r2 = reqwest::header::HeaderValue::from_str("https://courses.sjtu.edu.cn");
    println!("result2: {:?}", r2);
}
