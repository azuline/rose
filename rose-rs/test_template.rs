fn main() {
    let template = r#"
{% if disctotal > 1 %}{{ discnumber  < /dev/null |  zerofill(2) }}-{% endif %}{{ tracknumber | zerofill(2) }}.
{{ tracktitle }}
{% if trackartists.guest %}(feat. {{ trackartists.guest | artistsarrayfmt }}){% endif %}
"#;
    
    println\!("Template starts with newline: {}", template.starts_with('\n'));
    println\!("Template ends with newline: {}", template.ends_with('\n'));
    println\!("Template length: {}", template.len());
    println\!("Template bytes: {:?}", template.as_bytes());
}
