/// Image trace color mode for the extended Image Trace tool.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ImageTraceMode {
    #[default]
    Color,
    Grayscale,
    BlackWhite,
    Outlined,
}

/// Graph / chart type for the extended graph tool (Batch 8).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum GraphType {
    #[default]
    Column,
    Bar,
    Pie,
    Line,
    Scatter,
}

/// Graph data model: values, labels, and layout parameters.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphData {
    pub graph_type: GraphType,
    pub cols: usize,
    pub rows: usize,
    pub values: Vec<f32>,
    pub labels: Vec<String>,
}

impl Default for GraphData {
    fn default() -> Self {
        Self {
            graph_type: GraphType::Column,
            cols: 3,
            rows: 2,
            values: vec![10.0, 20.0, 30.0, 15.0, 25.0, 35.0],
            labels: vec![],
        }
    }
}
