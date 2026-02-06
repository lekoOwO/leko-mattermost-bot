use crate::constants::group_buy::*;
use crate::constants::validation::*;
use rust_decimal::Decimal;
use std::collections::HashMap;

/// 驗證錯誤類型
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    /// 欄位為空
    Empty { field: String },
    /// 欄位過長
    TooLong { field: String, max_length: usize },
    /// 價格超出範圍
    PriceOutOfRange { value: Decimal, max: i64 },
    /// 價格為負數
    NegativePrice { value: Decimal },
    /// 商品清單為空
    EmptyItems,
    /// YAML 格式錯誤
    InvalidYamlFormat { line: String },
    /// 價格格式錯誤
    InvalidPriceFormat { value: String },
    /// 自訂錯誤訊息
    Custom { message: String },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::Empty { field } => write!(f, "{}不能為空", field),
            ValidationError::TooLong { field, max_length } => {
                write!(f, "{}長度不能超過 {} 字元", field, max_length)
            }
            ValidationError::PriceOutOfRange { value, max } => {
                write!(f, "價格 {} 超出範圍 (最大: {})", value, max)
            }
            ValidationError::NegativePrice { value } => {
                write!(f, "價格不能為負數: {}", value)
            }
            ValidationError::EmptyItems => write!(f, "商品清單不能為空"),
            ValidationError::InvalidYamlFormat { line } => write!(f, "YAML 格式錯誤：{}", line),
            ValidationError::InvalidPriceFormat { value } => {
                write!(f, "價格格式錯誤：{}", value)
            }
            ValidationError::Custom { message } => write!(f, "{}", message),
        }
    }
}

impl std::error::Error for ValidationError {}

/// 團購表單驗證器
pub struct GroupBuyValidator;

impl GroupBuyValidator {
    /// 驗證商家名稱
    pub fn validate_merchant_name(name: &str) -> Result<String, ValidationError> {
        let trimmed = name.trim();
        
        if trimmed.is_empty() {
            return Err(ValidationError::Empty {
                field: "商家名稱".to_string(),
            });
        }
        
        if trimmed.len() > MAX_MERCHANT_NAME_LENGTH {
            return Err(ValidationError::TooLong {
                field: "商家名稱".to_string(),
                max_length: MAX_MERCHANT_NAME_LENGTH,
            });
        }
        
        Ok(trimmed.to_string())
    }

    /// 驗證描述（可選）
    pub fn validate_description(desc: Option<&str>) -> Result<Option<String>, ValidationError> {
        match desc {
            None => Ok(None),
            Some(s) => {
                let trimmed = s.trim();
                
                if trimmed.is_empty() {
                    return Ok(None);
                }
                
                if trimmed.len() > MAX_DESCRIPTION_LENGTH {
                    return Err(ValidationError::TooLong {
                        field: "描述".to_string(),
                        max_length: MAX_DESCRIPTION_LENGTH,
                    });
                }
                
                Ok(Some(trimmed.to_string()))
            }
        }
    }

    /// 驗證 metadata（可選）
    pub fn validate_metadata(metadata: Option<&str>) -> Result<Option<String>, ValidationError> {
        match metadata {
            None => Ok(None),
            Some(s) => {
                let trimmed = s.trim();
                
                if trimmed.is_empty() {
                    return Ok(None);
                }
                
                if trimmed.len() > MAX_METADATA_LENGTH {
                    return Err(ValidationError::TooLong {
                        field: "Metadata".to_string(),
                        max_length: MAX_METADATA_LENGTH,
                    });
                }
                
                Ok(Some(trimmed.to_string()))
            }
        }
    }

    /// 驗證調整說明（可選）
    pub fn validate_adjustments(adjustments: Option<&str>) -> Result<Option<String>, ValidationError> {
        match adjustments {
            None => Ok(None),
            Some(s) => {
                let trimmed = s.trim();
                
                if trimmed.is_empty() {
                    return Ok(None);
                }
                
                if trimmed.len() > MAX_ADJUSTMENTS_LENGTH {
                    return Err(ValidationError::TooLong {
                        field: "調整說明".to_string(),
                        max_length: MAX_ADJUSTMENTS_LENGTH,
                    });
                }
                
                Ok(Some(trimmed.to_string()))
            }
        }
    }

    /// 驗證單一價格
    pub fn validate_price(price: Decimal) -> Result<Decimal, ValidationError> {
        if price.is_sign_negative() {
            return Err(ValidationError::NegativePrice { value: price });
        }

        // 將 Decimal 轉換為整數比較（假設以分為單位）
        let price_cents = price.mantissa();
        if price_cents > MAX_PRICE as i128 {
            return Err(ValidationError::PriceOutOfRange {
                value: price,
                max: MAX_PRICE,
            });
        }

        Ok(price)
    }

    /// 驗證商品清單（從 YAML 字串解析）
    pub fn validate_items_yaml(yaml: &str) -> Result<HashMap<String, Decimal>, ValidationError> {
        let mut items = HashMap::new();

        for line in yaml.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Err(ValidationError::InvalidYamlFormat {
                    line: line.to_string(),
                });
            }

            let name = parts[0].trim();
            let price_str = parts[1].trim();

            if name.is_empty() {
                return Err(ValidationError::Empty {
                    field: "商品名稱".to_string(),
                });
            }

            let price = rust_decimal::Decimal::from_str_exact(price_str)
                .map_err(|_| ValidationError::InvalidPriceFormat {
                    value: price_str.to_string(),
                })?;

            Self::validate_price(price)?;

            items.insert(name.to_string(), price);
        }

        if items.is_empty() {
            return Err(ValidationError::EmptyItems);
        }

        Ok(items)
    }

    /// 驗證完整的團購表單資料
    pub fn validate_group_buy_form(
        merchant_name: &str,
        description: Option<&str>,
        metadata: Option<&str>,
        items_yaml: &str,
    ) -> Result<ValidatedGroupBuyForm, ValidationError> {
        let merchant_name = Self::validate_merchant_name(merchant_name)?;
        let description = Self::validate_description(description)?;
        let metadata = Self::validate_metadata(metadata)?;
        let items = Self::validate_items_yaml(items_yaml)?;

        Ok(ValidatedGroupBuyForm {
            merchant_name,
            description,
            metadata,
            items,
        })
    }
}

/// 已驗證的團購表單資料
#[derive(Debug, Clone)]
pub struct ValidatedGroupBuyForm {
    pub merchant_name: String,
    pub description: Option<String>,
    pub metadata: Option<String>,
    pub items: HashMap<String, Decimal>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    #[test]
    fn test_validate_merchant_name() {
        // 正常情況
        assert_eq!(
            GroupBuyValidator::validate_merchant_name("測試商家").unwrap(),
            "測試商家"
        );

        // 去除空白
        assert_eq!(
            GroupBuyValidator::validate_merchant_name("  測試商家  ").unwrap(),
            "測試商家"
        );

        // 空字串
        assert!(matches!(
            GroupBuyValidator::validate_merchant_name(""),
            Err(ValidationError::Empty { .. })
        ));

        // 過長
        let long_name = "a".repeat(MAX_MERCHANT_NAME_LENGTH + 1);
        assert!(matches!(
            GroupBuyValidator::validate_merchant_name(&long_name),
            Err(ValidationError::TooLong { .. })
        ));
    }

    #[test]
    fn test_validate_description() {
        // None
        assert_eq!(GroupBuyValidator::validate_description(None).unwrap(), None);

        // 正常情況
        assert_eq!(
            GroupBuyValidator::validate_description(Some("描述")).unwrap(),
            Some("描述".to_string())
        );

        // 空字串應返回 None
        assert_eq!(
            GroupBuyValidator::validate_description(Some("  ")).unwrap(),
            None
        );

        // 過長
        let long_desc = "a".repeat(MAX_DESCRIPTION_LENGTH + 1);
        assert!(matches!(
            GroupBuyValidator::validate_description(Some(&long_desc)),
            Err(ValidationError::TooLong { .. })
        ));
    }

    #[test]
    fn test_validate_price() {
        // 正常價格
        let price = Decimal::from_str("100.50").unwrap();
        assert!(GroupBuyValidator::validate_price(price).is_ok());

        // 零價格
        let zero = Decimal::from_str("0").unwrap();
        assert!(GroupBuyValidator::validate_price(zero).is_ok());

        // 負數價格
        let negative = Decimal::from_str("-10").unwrap();
        assert!(matches!(
            GroupBuyValidator::validate_price(negative),
            Err(ValidationError::NegativePrice { .. })
        ));

        // 超出範圍的價格
        let too_large = Decimal::from_str("100001").unwrap();
        assert!(matches!(
            GroupBuyValidator::validate_price(too_large),
            Err(ValidationError::PriceOutOfRange { .. })
        ));
    }

    #[test]
    fn test_validate_items_yaml() {
        // 正常情況
        let yaml = "商品A: 100\n商品B: 200.5\n";
        let items = GroupBuyValidator::validate_items_yaml(yaml).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items.get("商品A").unwrap(), &Decimal::from_str("100").unwrap());
        assert_eq!(items.get("商品B").unwrap(), &Decimal::from_str("200.5").unwrap());

        // 含註解和空行
        let yaml = "# 註解\n商品A: 100\n\n商品B: 200\n";
        let items = GroupBuyValidator::validate_items_yaml(yaml).unwrap();
        assert_eq!(items.len(), 2);

        // 空清單
        assert!(matches!(
            GroupBuyValidator::validate_items_yaml(""),
            Err(ValidationError::EmptyItems)
        ));

        // 格式錯誤
        assert!(matches!(
            GroupBuyValidator::validate_items_yaml("商品A 100"),
            Err(ValidationError::InvalidYamlFormat { .. })
        ));

        // 價格格式錯誤
        assert!(matches!(
            GroupBuyValidator::validate_items_yaml("商品A: abc"),
            Err(ValidationError::InvalidPriceFormat { .. })
        ));

        // 負數價格
        assert!(matches!(
            GroupBuyValidator::validate_items_yaml("商品A: -100"),
            Err(ValidationError::NegativePrice { .. })
        ));
    }

    #[test]
    fn test_validate_group_buy_form() {
        let yaml = "商品A: 100\n商品B: 200\n";
        let result = GroupBuyValidator::validate_group_buy_form(
            "測試商家",
            Some("測試描述"),
            None,
            yaml,
        );

        assert!(result.is_ok());
        let form = result.unwrap();
        assert_eq!(form.merchant_name, "測試商家");
        assert_eq!(form.description, Some("測試描述".to_string()));
        assert_eq!(form.metadata, None);
        assert_eq!(form.items.len(), 2);
    }

    #[test]
    fn test_validate_metadata() {
        // None
        assert_eq!(GroupBuyValidator::validate_metadata(None).unwrap(), None);

        // 正常情況
        assert_eq!(
            GroupBuyValidator::validate_metadata(Some("metadata")).unwrap(),
            Some("metadata".to_string())
        );

        // 過長
        let long_meta = "a".repeat(MAX_METADATA_LENGTH + 1);
        assert!(matches!(
            GroupBuyValidator::validate_metadata(Some(&long_meta)),
            Err(ValidationError::TooLong { .. })
        ));
    }

    #[test]
    fn test_validate_adjustments() {
        // None
        assert_eq!(GroupBuyValidator::validate_adjustments(None).unwrap(), None);

        // 正常情況
        assert_eq!(
            GroupBuyValidator::validate_adjustments(Some("調整說明")).unwrap(),
            Some("調整說明".to_string())
        );

        // 過長
        let long_adj = "a".repeat(MAX_ADJUSTMENTS_LENGTH + 1);
        assert!(matches!(
            GroupBuyValidator::validate_adjustments(Some(&long_adj)),
            Err(ValidationError::TooLong { .. })
        ));
    }
}
