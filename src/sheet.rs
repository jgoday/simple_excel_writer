use std::io::{Result, Write};

#[macro_export]
macro_rules! row {
    ($( $x:expr ),*) => {
        {
            let mut row = Row::new();
            $(row.add_cell($x);)*
                row
        }
    };
}

#[macro_export]
macro_rules! blank {
    ($x:expr) => {{
        CellValue::Blank($x)
    }};
    () => {{
        CellValue::Blank(1)
    }};
}

#[derive(Default)]
pub struct Sheet {
    pub id: usize,
    pub name: String,
    pub columns: Vec<Column>,
    max_row_index: usize,
    merged_cells: Vec<MergedCell>,
}

#[derive(Default, Clone)]
pub struct MergedCell {
    pub column_index: usize,
    pub row_index: usize,
    pub column_span: usize,
    pub row_span: usize,
}

#[derive(Default)]
pub struct Row {
    pub cells: Vec<Cell>,
    row_index: usize,
    max_col_index: usize,
}

pub struct Cell {
    pub column_index: usize,
    pub value: CellValue,
}

pub struct Column {
    pub width: f32,
}

#[derive(Clone)]
pub enum CellValue {
    Bool(bool),
    Number(f64),
    String(String),
    Blank(usize),
    SharedString(String),
}

pub struct SheetWriter<'a, 'b>
where
    'b: 'a,
{
    sheet: &'a mut Sheet,
    writer: &'b mut Vec<u8>,
    shared_strings: &'b mut crate::SharedStrings,
}

pub trait ToCellValue {
    fn to_cell_value(&self) -> CellValue;
}

impl ToCellValue for bool {
    fn to_cell_value(&self) -> CellValue {
        CellValue::Bool(self.to_owned())
    }
}

impl ToCellValue for f64 {
    fn to_cell_value(&self) -> CellValue {
        CellValue::Number(self.to_owned())
    }
}

impl ToCellValue for String {
    fn to_cell_value(&self) -> CellValue {
        CellValue::String(self.to_owned())
    }
}

impl<'a> ToCellValue for &'a str {
    fn to_cell_value(&self) -> CellValue {
        CellValue::String(self.to_owned().to_owned())
    }
}

impl ToCellValue for () {
    fn to_cell_value(&self) -> CellValue {
        CellValue::Blank(1)
    }
}

impl Row {
    pub fn new() -> Row {
        Row {
            ..Default::default()
        }
    }

    pub fn add_cell<T>(&mut self, value: T)
    where
        T: ToCellValue + Sized,
    {
        let value = value.to_cell_value();
        match value {
            CellValue::Blank(cols) => self.max_col_index += cols,
            _ => {
                self.max_col_index += 1;
                self.cells.push(Cell {
                    column_index: self.max_col_index,
                    value,
                })
            }
        }
    }

    pub fn add_empty_cells(&mut self, cols: usize) {
        self.max_col_index += cols
    }

    pub fn join(&mut self, row: Row) {
        for cell in row.cells.into_iter() {
            self.inner_add_cell(cell)
        }
    }

    fn inner_add_cell(&mut self, cell: Cell) {
        self.max_col_index += 1;
        self.cells.push(Cell {
            column_index: self.max_col_index,
            value: cell.value,
        })
    }

    pub fn write(&mut self, writer: &mut dyn Write, merged_cells: &Vec<MergedCell>) -> Result<()> {
        let head = format!("<row r=\"{}\">\n", self.row_index);
        writer.write_all(head.as_bytes())?;
        for c in self.cells.iter() {
            let prev_col_cells: Vec<&MergedCell> = merged_cells
                .iter()
                .filter(|mc| mc.column_index < c.column_index)
                .collect();
            let prev_col_span = prev_col_cells
                .iter()
                .fold(0, |a, cell| a + cell.column_span);
            c.write(self.row_index, writer, prev_col_span)?;
        }
        writer.write_all(b"\n</row>\n")
    }

    pub fn replace_strings(mut self, shared: &mut crate::SharedStrings) -> Self {
        if !shared.used() {
            return self;
        }
        for cell in self.cells.iter_mut() {
            cell.value = match &cell.value {
                CellValue::String(val) => shared.register(&escape_xml(val)),
                x => x.to_owned(),
            };
        }
        self
    }
}

impl ToCellValue for CellValue {
    fn to_cell_value(&self) -> CellValue {
        self.clone()
    }
}

fn write_value(cv: &CellValue, ref_id: String, writer: &mut dyn Write) -> Result<()> {
    match cv {
        CellValue::Bool(b) => {
            let v = if *b { 1 } else { 0 };
            let s = format!("<c r=\"{}\" t=\"b\"><v>{}</v></c>", ref_id, v);
            writer.write_all(s.as_bytes())?;
        }
        CellValue::Number(num) => {
            let s = format!("<c r=\"{}\" ><v>{}</v></c>", ref_id, num);
            writer.write_all(s.as_bytes())?;
        }
        CellValue::String(ref s) => {
            let s = format!(
                "<c r=\"{}\" t=\"str\"><v>{}</v></c>",
                ref_id,
                escape_xml(&s)
            );
            writer.write_all(s.as_bytes())?;
        }
        CellValue::SharedString(ref s) => {
            let s = format!("<c r=\"{}\" t=\"s\"><v>{}</v></c>", ref_id, s);
            writer.write_all(s.as_bytes())?;
        }
        CellValue::Blank(_) => {}
    }
    Ok(())
}

fn escape_xml(str: &str) -> String {
    let str = str.replace("&", "&amp;");
    let str = str.replace("<", "&lt;");
    let str = str.replace(">", "&gt;");
    let str = str.replace("'", "&apos;");
    str.replace("\"", "&quot;")
}

impl Cell {
    fn write(&self, row_index: usize, writer: &mut dyn Write, prev_col_span: usize) -> Result<()> {
        let ref_id = format!(
            "{}{}",
            column_letter(self.column_index + prev_col_span),
            row_index
        );
        write_value(&self.value, ref_id, writer)
    }
}

/**
 * column_index : 1-based
 */
pub fn column_letter(column_index: usize) -> String {
    let mut column_index = (column_index - 1) as isize; // turn to 0-based;
    let single = |n: u8| {
        // n : 0-based
        (b'A' + n) as char
    };
    let mut result = vec![];
    while column_index >= 0 {
        result.push(single((column_index % 26) as u8));
        column_index = column_index / 26 - 1;
    }

    let result = result.into_iter().rev();

    use std::iter::FromIterator;
    String::from_iter(result)
}

pub fn validate_name(name: &str) -> String {
    let mut name = escape_xml(name);
    let boundary = if name.is_char_boundary(30) { 30 } else { 29 };
    name.truncate(boundary);
    name.replace("/", "-")
}

impl Sheet {
    pub fn new(id: usize, sheet_name: &str) -> Sheet {
        Sheet {
            id,
            name: validate_name(sheet_name), //sheet_name.to_owned(),//escape_xml(sheet_name),
            ..Default::default()
        }
    }

    pub fn add_column(&mut self, column: Column) {
        self.columns.push(column)
    }

    fn write_row<W>(
        &mut self,
        writer: &mut W,
        mut row: Row,
        merged_cells: &Vec<MergedCell>,
    ) -> Result<()>
    where
        W: Write + Sized,
    {
        self.max_row_index += 1;
        row.row_index = self.max_row_index;
        row.write(writer, merged_cells)
    }

    fn write_blank_rows(&mut self, rows: usize) {
        self.max_row_index += rows;
    }

    fn write_head(&self, writer: &mut dyn Write) -> Result<()> {
        let header = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
        xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
        "#;
        writer.write_all(header.as_bytes())?;
        /*
        let dimension = format!("<dimension ref=\"A1:{}{}\"/>", column_letter(self.dimension.columns), self.dimension.rows);
        writer.write_all(dimension.as_bytes())?;
        */

        if self.columns.is_empty() {
            return Ok(());
        }

        writer.write_all(b"\n<cols>\n")?;
        let mut i = 1;
        for col in self.columns.iter() {
            writer.write_all(
                format!(
                    "<col min=\"{}\" max=\"{}\" width=\"{}\" customWidth=\"1\"/>\n",
                    &i, &i, col.width
                )
                .as_bytes(),
            )?;
            i += 1;
        }
        writer.write_all(b"</cols>\n")
    }

    fn write_merged_cells(&self, writer: &mut dyn Write) -> Result<()> {
        if self.merged_cells.len() > 0 {
            writer.write_all(
                format!("\n<mergeCells count=\"{}\">", self.merged_cells.len()).as_bytes(),
            )?;

            for mc in self.merged_cells.iter() {
                let start = format!("{}{}", column_letter(mc.column_index), mc.row_index);
                let end = format!(
                    "{}{}",
                    column_letter(mc.column_index + mc.column_span),
                    mc.row_index + mc.row_span
                );

                writer
                    .write_all(format!("\n\t<mergeCell ref=\"{}:{}\" />", start, end).as_bytes())?;
            }

            writer.write_all(b"\n</mergeCells>\n")
        } else {
            Ok(())
        }
    }

    fn write_data_begin(&self, writer: &mut dyn Write) -> Result<()> {
        writer.write_all(b"\n<sheetData>\n")
    }

    fn write_data_end(&self, writer: &mut dyn Write) -> Result<()> {
        writer.write_all(b"\n</sheetData>\n")
    }

    fn close(&self, writer: &mut dyn Write) -> Result<()> {
        writer.write_all(b"</worksheet>\n")
    }
}

impl<'a, 'b> SheetWriter<'a, 'b> {
    pub fn new(
        sheet: &'a mut Sheet,
        writer: &'b mut Vec<u8>,
        shared_strings: &'b mut crate::SharedStrings,
    ) -> SheetWriter<'a, 'b> {
        SheetWriter {
            sheet,
            writer,
            shared_strings,
        }
    }

    pub fn merge_cells(
        &mut self,
        column_index: usize,
        row_index: usize,
        column_span: usize,
        row_span: usize,
    ) {
        self.sheet.merged_cells.push(MergedCell {
            column_index,
            row_index,
            column_span,
            row_span,
        });
    }

    pub fn append_row(&mut self, row: Row) -> Result<()> {
        let merged_cells: Vec<MergedCell> = self
            .sheet
            .merged_cells
            .iter()
            .filter(|x| {
                x.row_index <= self.sheet.max_row_index + 1
                    && x.row_index + x.row_span >= self.sheet.max_row_index + 1
            })
            .map(|x| x.clone())
            .collect();
        self.sheet.write_row(
            self.writer,
            row.replace_strings(&mut self.shared_strings),
            &merged_cells,
        )
    }

    pub fn append_blank_rows(&mut self, rows: usize) {
        self.sheet.write_blank_rows(rows)
    }

    pub fn write<F>(&mut self, write_data: F) -> Result<()>
    where
        F: FnOnce(&mut SheetWriter) -> Result<()> + Sized,
    {
        self.sheet.write_head(self.writer)?;

        self.sheet.write_data_begin(self.writer)?;

        write_data(self)?;

        self.sheet.write_data_end(self.writer)?;
        self.sheet.write_merged_cells(self.writer)?;
        self.sheet.close(self.writer)
    }
}
