use std::error::Error;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartFormat {
    OsuMania,
    StepMania,
}

impl ChartFormat {
    pub fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value.to_ascii_lowercase().as_str() {
            "osu" | "osu-mania" | "mania" => Ok(Self::OsuMania),
            "sm" | "stepmania" => Ok(Self::StepMania),
            "auto" => Err("auto is only valid as an option default".into()),
            _ => Err(format!("unknown chart format: {value}").into()),
        }
    }

    pub fn detect(path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let path = path.as_ref();
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("osu") => Ok(Self::OsuMania),
            Some("sm") => Ok(Self::StepMania),
            _ => Err(format!(
                "could not detect chart format from path: {}",
                path.display()
            )
            .into()),
        }
    }
}

pub fn parse_chart_format_option(value: &str) -> Result<Option<ChartFormat>, Box<dyn Error>> {
    if value.eq_ignore_ascii_case("auto") {
        return Ok(None);
    }

    Ok(Some(ChartFormat::parse(value)?))
}

#[cfg(test)]
mod tests {
    use super::{ChartFormat, parse_chart_format_option};

    #[test]
    fn detects_chart_format_from_extension() {
        assert_eq!(
            ChartFormat::detect("song.osu").unwrap(),
            ChartFormat::OsuMania
        );
        assert_eq!(
            ChartFormat::detect("song.sm").unwrap(),
            ChartFormat::StepMania
        );
    }

    #[test]
    fn parses_auto_as_no_forced_format() {
        assert_eq!(parse_chart_format_option("auto").unwrap(), None);
        assert_eq!(
            parse_chart_format_option("stepmania").unwrap(),
            Some(ChartFormat::StepMania)
        );
    }
}
