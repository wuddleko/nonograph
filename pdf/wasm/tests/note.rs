use nonograph_pdf::note_pdf;

#[test]
fn the_pdf_export_returns_the_note_pdf() {
    let html = "<p>Hello</p>";
    let bytes = note_pdf("Title", "Ada", html).unwrap();
    assert_eq!(bytes, pdf::note_pdf("Title", "Ada", html).unwrap());
    assert!(bytes.starts_with(b"%PDF-1.4\n"));
    assert_eq!(
        note_pdf("", "", "").unwrap(),
        pdf::note_pdf("", "", "").unwrap()
    );
}
