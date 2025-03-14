use anyhow::Result;
use iris::font::grammar::{Glyph, GlyphData};
use iris::font::shaper::TrueTypeFontShaper;
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

fn draw_glyph_to_canvas(glyph: &Glyph, key: usize) -> Result<String> {
    let GlyphData::Simple(simple_glyph) = &glyph.data else {
        todo!("how does compound glyphs look on canvas?");
    };

    let mut out = String::new();
    out += &format!("ctx{key}.translate(0, newCanvas{key}.height - 300);\n");
    out += &format!("ctx{key}.scale(0.5, -0.5);\n");

    out += &format!("ctx{key}.beginPath()\n");

    let mut implicit_on_curve_points = vec![];

    let mut start_index = 0;

    for end_index in &simple_glyph.end_points_of_contours {
        let end_index = *end_index as usize;

        let mut i = start_index;

        let (x, y) = simple_glyph.coordinates[i];
        out += &format!("ctx{key}.moveTo({}, {});\n", x, y);

        i += 1;

        while i <= end_index {
            // let (prev_x, prev_y) = simple_glyph.coordinates[i - 1];
            let prev_on_curve = simple_glyph.on_curve(i - 1);

            let (curr_x, curr_y) = simple_glyph.coordinates[i];
            let start_on_curve = simple_glyph.on_curve(i);

            match (prev_on_curve, start_on_curve) {
                (true, true) => {
                    // // an implicit off-curve point.
                    // out += &format!(
                    //     "ctx{key}.quadraticCurveTo({}, {}, {}, {});\n",
                    //     mid_x, mid_y, curr_x, curr_y,
                    // );

                    implicit_on_curve_points.push(simple_glyph.interpolate_with_prev(i)?);

                    out += &format!("ctx{key}.lineTo({}, {});\n", curr_x, curr_y);
                }
                _ => {
                    out += &format!("ctx{key}.lineTo({}, {});\n", curr_x, curr_y);
                }
            }

            i += 1
        }

        out += &format!("ctx{key}.closePath();\n");

        start_index = end_index + 1;
    }

    out += &format!("ctx{key}.lineWidth = 9;\n");
    out += &format!("ctx{key}.stroke();\n");

    Ok(out)
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
    let args = std::env::args().skip(1);
    let phrase = args.collect::<Vec<String>>().join(" ");

    let ttf_file = fs::read("./src/font/lato-regular.ttf")?;
    let ttf = TrueTypeFontParser::new(&ttf_file).parse()?;
    let shaper = TrueTypeFontShaper::from(&ttf);

    let glyphs = shaper.shape(&phrase);

    let mut render_js_code = String::new();
    render_js_code += "const contentDiv = document.getElementById(\"content\")\n";

    for (i, glyph) in glyphs.into_iter().enumerate() {
        if !glyph.is_simple() {
            continue;
        }

        render_js_code += &dom_new_canvas(i, glyph.description.width(), glyph.description.height());
        render_js_code += &format!("const ctx{} = newCanvas{}.getContext(\"2d\");\n", i, i);
        render_js_code += &draw_glyph_to_canvas(glyph, i)?;
        render_js_code += &format!("contentDiv.appendChild(newCanvas{});\n\n", i);
    }

    let dir_path = Path::new("glyph_playground");
    fs::create_dir_all(dir_path)?;

    let mut file = File::create("glyph_playground/glyph.js")?;
    file.write_all(render_js_code.as_bytes())?;

    let mut file = File::create("glyph_playground/index.html")?;
    file.write_all(index_html().as_bytes())?;

    let abs_path = fs::canonicalize(dir_path.join("index.html"))?;
    println!("Done!\tfile://{}", abs_path.display());

    Ok(())
}
