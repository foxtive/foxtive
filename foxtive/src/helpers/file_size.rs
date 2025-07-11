use std::fmt;

/// Represents different size units and their corresponding values
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeUnit {
    Byte,
    Kilobyte,
    Megabyte,
    Gigabyte,
    Terabyte,
    Petabyte,
    Exabyte,
}

impl SizeUnit {
    /// Returns the value in bytes for this unit
    pub const fn bytes(self) -> u64 {
        match self {
            Self::Byte => 1,
            Self::Kilobyte => 1_024,
            Self::Megabyte => 1_024_u64.pow(2),
            Self::Gigabyte => 1_024_u64.pow(3),
            Self::Terabyte => 1_024_u64.pow(4),
            Self::Petabyte => 1_024_u64.pow(5),
            Self::Exabyte => 1_024_u64.pow(6),
        }
    }

    /// Returns the abbreviated unit string
    pub const fn abbrev(self) -> &'static str {
        match self {
            Self::Byte => "B",
            Self::Kilobyte => "KB",
            Self::Megabyte => "MB",
            Self::Gigabyte => "GB",
            Self::Terabyte => "TB",
            Self::Petabyte => "PB",
            Self::Exabyte => "EB",
        }
    }

    /// Returns the full unit name
    pub const fn name(self) -> &'static str {
        match self {
            Self::Byte => "byte",
            Self::Kilobyte => "kilobyte",
            Self::Megabyte => "megabyte",
            Self::Gigabyte => "gigabyte",
            Self::Terabyte => "terabyte",
            Self::Petabyte => "petabyte",
            Self::Exabyte => "exabyte",
        }
    }

    /// Returns the plural form of the unit name
    pub const fn plural_name(self) -> &'static str {
        match self {
            Self::Byte => "bytes",
            Self::Kilobyte => "kilobytes",
            Self::Megabyte => "megabytes",
            Self::Gigabyte => "gigabytes",
            Self::Terabyte => "terabytes",
            Self::Petabyte => "petabytes",
            Self::Exabyte => "exabytes",
        }
    }
}

/// Configuration for size formatting
#[derive(Debug, Clone)]
pub struct SizeFormatConfig {
    pub precision: usize,
    pub use_full_names: bool,
    pub use_plural: bool,
    pub use_binary_prefix: bool,
    pub min_unit: SizeUnit,
    pub max_unit: SizeUnit,
    pub separator: String,
    pub show_exact_bytes: bool,
}

impl Default for SizeFormatConfig {
    fn default() -> Self {
        Self {
            precision: 2,
            use_full_names: false,
            use_plural: false,
            use_binary_prefix: true,
            min_unit: SizeUnit::Byte,
            max_unit: SizeUnit::Exabyte,
            separator: " ".to_string(),
            show_exact_bytes: false,
        }
    }
}

impl SizeFormatConfig {
    /// Creates a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the decimal precision for formatted output
    pub fn precision(mut self, precision: usize) -> Self {
        self.precision = precision;
        self
    }

    /// Use full unit names instead of abbreviations
    pub fn full_names(mut self) -> Self {
        self.use_full_names = true;
        self
    }

    /// Use plural forms for full names when value != 1
    pub fn plural(mut self) -> Self {
        self.use_plural = true;
        self
    }

    /// Use decimal (1000) instead of binary (1024) prefixes
    pub fn decimal_prefix(mut self) -> Self {
        self.use_binary_prefix = false;
        self
    }

    /// Set minimum unit to display
    pub fn min_unit(mut self, unit: SizeUnit) -> Self {
        self.min_unit = unit;
        self
    }

    /// Set maximum unit to display
    pub fn max_unit(mut self, unit: SizeUnit) -> Self {
        self.max_unit = unit;
        self
    }

    /// Set separator between number and unit
    pub fn separator<S: Into<String>>(mut self, sep: S) -> Self {
        self.separator = sep.into();
        self
    }

    /// Show exact byte count in parentheses for large sizes
    pub fn show_exact_bytes(mut self) -> Self {
        self.show_exact_bytes = true;
        self
    }
}

/// Represents a formatted size with its value and unit
#[derive(Debug, Clone, PartialEq)]
pub struct FormattedSize {
    pub value: f64,
    pub unit: SizeUnit,
    pub original_bytes: u64,
}

impl FormattedSize {
    /// Creates a new FormattedSize
    pub fn new(bytes: u64, config: &SizeFormatConfig) -> Self {
        let unit = Self::determine_best_unit(bytes, config);
        let divisor = if config.use_binary_prefix {
            unit.bytes() as f64
        } else {
            // For decimal prefixes, use powers of 1000
            match unit {
                SizeUnit::Byte => 1.0,
                SizeUnit::Kilobyte => 1000.0,
                SizeUnit::Megabyte => 1000.0_f64.powi(2),
                SizeUnit::Gigabyte => 1000.0_f64.powi(3),
                SizeUnit::Terabyte => 1000.0_f64.powi(4),
                SizeUnit::Petabyte => 1000.0_f64.powi(5),
                SizeUnit::Exabyte => 1000.0_f64.powi(6),
            }
        };

        let value = bytes as f64 / divisor;

        Self {
            value,
            unit,
            original_bytes: bytes,
        }
    }

    /// Determines the best unit for displaying the given byte count
    fn determine_best_unit(bytes: u64, config: &SizeFormatConfig) -> SizeUnit {
        let units = [
            SizeUnit::Exabyte,
            SizeUnit::Petabyte,
            SizeUnit::Terabyte,
            SizeUnit::Gigabyte,
            SizeUnit::Megabyte,
            SizeUnit::Kilobyte,
            SizeUnit::Byte,
        ];

        // let base = if config.use_binary_prefix { 1024 } else { 1000 };

        for &unit in &units {
            let threshold = if config.use_binary_prefix {
                unit.bytes()
            } else {
                match unit {
                    SizeUnit::Byte => 1,
                    SizeUnit::Kilobyte => 1000,
                    SizeUnit::Megabyte => 1000_u64.pow(2),
                    SizeUnit::Gigabyte => 1000_u64.pow(3),
                    SizeUnit::Terabyte => 1000_u64.pow(4),
                    SizeUnit::Petabyte => 1000_u64.pow(5),
                    SizeUnit::Exabyte => 1000_u64.pow(6),
                }
            };

            if bytes >= threshold && unit as u8 >= config.min_unit as u8 && unit as u8 <= config.max_unit as u8 {
                return unit;
            }
        }

        config.min_unit
    }

    /// Formats the size according to the given configuration
    pub fn format(&self, config: &SizeFormatConfig) -> String {
        let unit_str = if config.use_full_names {
            if config.use_plural && (self.value != 1.0 || self.unit == SizeUnit::Byte && self.original_bytes != 1) {
                self.unit.plural_name()
            } else {
                self.unit.name()
            }
        } else {
            self.unit.abbrev()
        };

        let formatted_value = if self.unit == SizeUnit::Byte {
            format!("{}", self.original_bytes)
        } else {
            format!("{:.prec$}", self.value, prec = config.precision)
        };

        let mut result = format!("{}{}{}", formatted_value, config.separator, unit_str);

        if config.show_exact_bytes && self.unit != SizeUnit::Byte && self.original_bytes >= 1024 {
            result.push_str(&format!(" ({} bytes)", self.original_bytes));
        }

        result
    }
}

impl fmt::Display for FormattedSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let config = SizeFormatConfig::default();
        write!(f, "{}", self.format(&config))
    }
}

/// Advanced size formatter with extensive configuration options
pub struct SizeFormatter {
    config: SizeFormatConfig,
}

impl SizeFormatter {
    /// Creates a new SizeFormatter with default configuration
    pub fn new() -> Self {
        Self {
            config: SizeFormatConfig::default(),
        }
    }

    /// Creates a new SizeFormatter with custom configuration
    pub fn with_config(config: SizeFormatConfig) -> Self {
        Self { config }
    }

    /// Formats a size in bytes using the configured settings
    pub fn format(&self, bytes: u64) -> String {
        let formatted = FormattedSize::new(bytes, &self.config);
        formatted.format(&self.config)
    }

    /// Formats a size and returns the FormattedSize struct for further processing
    pub fn format_detailed(&self, bytes: u64) -> FormattedSize {
        FormattedSize::new(bytes, &self.config)
    }

    /// Updates the configuration
    pub fn set_config(&mut self, config: SizeFormatConfig) {
        self.config = config;
    }

    /// Gets a reference to the current configuration
    pub fn config(&self) -> &SizeFormatConfig {
        &self.config
    }
}

impl Default for SizeFormatter {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function for quick size formatting with default settings
pub fn format_size(size_in_bytes: u64) -> String {
    SizeFormatter::new().format(size_in_bytes)
}

/// Convenience function for size formatting with custom precision
pub fn format_size_with_precision(size_in_bytes: u64, precision: usize) -> String {
    let config = SizeFormatConfig::new().precision(precision);
    SizeFormatter::with_config(config).format(size_in_bytes)
}

/// Convenience function for size formatting with full unit names
pub fn format_size_verbose(size_in_bytes: u64) -> String {
    let config = SizeFormatConfig::new().full_names().plural();
    SizeFormatter::with_config(config).format(size_in_bytes)
}

/// Convenience function for size formatting with decimal prefixes (1000-based)
pub fn format_size_decimal(size_in_bytes: u64) -> String {
    let config = SizeFormatConfig::new().decimal_prefix();
    SizeFormatter::with_config(config).format(size_in_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_formatting() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(1048576), "1.00 MB");
        assert_eq!(format_size(1073741824), "1.00 GB");
        assert_eq!(format_size(1449551462), "1.35 GB");
    }

    #[test]
    fn test_precision() {
        assert_eq!(format_size_with_precision(1536, 0), "2 KB");
        assert_eq!(format_size_with_precision(1536, 1), "1.5 KB");
        assert_eq!(format_size_with_precision(1536, 3), "1.500 KB");
    }

    #[test]
    fn test_verbose_formatting() {
        assert_eq!(format_size_verbose(1), "1 byte");
        assert_eq!(format_size_verbose(2), "2 bytes");
        assert_eq!(format_size_verbose(1024), "1.00 kilobyte");
        assert_eq!(format_size_verbose(1048576), "1.00 megabyte");
        assert_eq!(format_size_verbose(1536), "1.50 kilobytes");
        assert_eq!(format_size_verbose(2097152), "2.00 megabytes");
    }

    #[test]
    fn test_decimal_prefixes() {
        assert_eq!(format_size_decimal(1000), "1.00 KB");
        assert_eq!(format_size_decimal(1000000), "1.00 MB");
        assert_eq!(format_size_decimal(1000000000), "1.00 GB");
    }

    #[test]
    fn test_large_sizes() {
        let config = SizeFormatConfig::new().show_exact_bytes();
        let formatter = SizeFormatter::with_config(config);

        let result = formatter.format(1099511627776); // 1 TB
        assert!(result.contains("1.00 TB"));
        assert!(result.contains("(1099511627776 bytes)"));
    }

    #[test]
    fn test_unit_constraints() {
        let config = SizeFormatConfig::new()
            .min_unit(SizeUnit::Kilobyte)
            .max_unit(SizeUnit::Megabyte);

        let formatter = SizeFormatter::with_config(config);

        assert_eq!(formatter.format(512), "0.50 KB"); // Below min, uses min
        assert_eq!(formatter.format(1073741824), "1024.00 MB"); // Above max, uses max
    }

    #[test]
    fn test_custom_separator() {
        let config = SizeFormatConfig::new().separator("_");
        let formatter = SizeFormatter::with_config(config);

        assert_eq!(formatter.format(1024), "1.00_KB");
    }

    #[test]
    fn test_edge_cases() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(u64::MAX), format!("{:.2} EB", u64::MAX as f64 / SizeUnit::Exabyte.bytes() as f64));
    }
}