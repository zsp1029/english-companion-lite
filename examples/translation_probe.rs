// Sends only this fixed, non-sensitive sentence to the selected free service.
// No application text, clipboard, credentials, or input history is collected.
#[cfg(windows)]
fn main() {
    use english_companion_lite::translator::{winhttp::MymemoryTranslator, Translator};
    match (MymemoryTranslator {
        email: String::new(),
    })
    .translate("今天工作比较忙，晚上想早点休息。")
    {
        Ok(text) => println!("MyMemory zh-CN -> en: {text}"),
        Err(error) => {
            eprintln!("{}", error.status());
            std::process::exit(1);
        }
    }
}
#[cfg(not(windows))]
fn main() {
    eprintln!("This probe requires Windows WinHTTP.");
}
