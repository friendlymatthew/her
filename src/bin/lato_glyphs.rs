use anyhow::Result;
use iris::font::TrueTypeFontParser;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;

fn dom_new_canvas(i: usize, width: usize, height: usize) -> String {
    let mut out = String::new();
    out += &format!(
        "const newCanvas{} = document.createElement(\"canvas\");\n",
        i
    );
    out += &format!("newCanvas{}.width = {};\n", i, width);
    out += &format!("newCanvas{}.height = {};\n", i, height);

    out
}

fn index_html() -> String {
    let mut out = String::new();
    out += r#"
<!-- Don't touch this! It's autogenerated! -->
<html>
    <head>
        <meta content="text/html;charset=utf-8" http-equiv="Content-Type" />
    </head>
    <body>
        <h1>Glyph Playground</h1>
        <div id="content"></div>
        <script src="glyph.js"></script>
    </body>
</html>
"#;

    out
}

fn main() -> Result<()> {
    let ttf_file = fs::read("./src/font/lato-regular.ttf")?;
    let ttf = TrueTypeFontParser::new(&ttf_file).parse()?;

    let mut render_js_code = String::new();
    render_js_code += "const contentDiv = document.getElementById(\"content\")\n";

    for (i, glyph) in ttf.glyph_table.glyphs.iter().enumerate() {
        if !glyph.is_simple() {
            continue;
        }

        render_js_code += &dom_new_canvas(i, glyph.description.width(), glyph.description.height());
        render_js_code += &format!("const ctx{} = newCanvas{}.getContext(\"2d\");\n", i, i);

        render_js_code += &glyph.draw_to_canvas(i);
        render_js_code += &format!("contentDiv.appendChild(newCanvas{});\n\n", i);
    }

    let dir_path = Path::new("glyph_playground");
    fs::create_dir_all(dir_path)?;

    let mut file = File::create("glyph_playground/glyph.js")?;
    file.write_all(render_js_code.as_bytes())?;

    let mut file = File::create("glyph_playground/index.html")?;
    file.write_all(index_html().as_bytes())?;

    Ok(())
}
