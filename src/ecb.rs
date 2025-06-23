use chrono;
use roxmltree;

pub fn get_eur_to_usd_exchange_rate(
    start_date: chrono::NaiveDate,
    end_date: chrono::NaiveDate,
) -> Result<f32, String> {
    let query = [
        ("startPeriod", start_date.format("%Y-%m-%d").to_string()),
        ("endPeriod", end_date.format("%Y-%m-%d").to_string()),
    ];
    let response: String =
        get_blocking_exchange_rate(ECB_URL, &query).expect("Failed to get exchange rate");
    let ecb_response = EcbResponse::from_xml_string(&response).unwrap();
    assert_eq!(ecb_response.currency, "USD");
    assert_eq!(ecb_response.currency_denom, "EUR");
    let usd_to_eur = ecb_response
        .rate
        .parse::<f32>()
        .map_err(|e| format!("Failed to parse exchange rate: {}", e))?;
    invert_exchange_rate(usd_to_eur)
}

fn invert_exchange_rate(rate: f32) -> Result<f32, String> {
    if rate == 0.0 {
        return Err("Rate is zero".to_string());
    }
    Ok(1.0 / rate)
}

const ECB_URL: &str = "https://data-api.ecb.europa.eu/service/data/EXR/D.USD.EUR.SP00.A";

fn get_blocking_exchange_rate<T>(url: &str, query: &T) -> Result<String, String>
where
    T: serde::Serialize + ?Sized,
{
    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|e| format!("Failed to build client: {}", e))?;

    let response = client
        .get(url)
        .query(query)
        .send()
        .map_err(|e| format!("Failed to send request: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "Request failed with status {}: {}",
            status,
            response.text().unwrap_or_default()
        ));
    }

    let content_type = response
        .headers()
        .get("content-type")
        .ok_or("Content-Type header missing")?
        .to_str()
        .map_err(|e| format!("Failed to convert Content-Type header to string: {}", e))?;

    let expected_content_type = "application/vnd.sdmx.genericdata+xml;version=2.1";
    if content_type != expected_content_type {
        return Err(format!(
            "Unexpected Content-Type: {}, expected: {}",
            content_type, expected_content_type
        ));
    }

    response
        .text()
        .map_err(|e| format!("Failed to read response text: {}", e))
}

struct EcbResponse {
    #[allow(dead_code)]
    sender_id: String,
    #[allow(dead_code)]
    urn: String,
    #[allow(dead_code)]
    freq: String,
    currency: String,
    currency_denom: String,
    #[allow(dead_code)]
    title: String,
    #[allow(dead_code)]
    unit: String,
    #[allow(dead_code)]
    date: String,
    rate: String,
}

impl EcbResponse {
    pub fn from_xml_string(xml: &str) -> Result<Self, String> {
        let opt = roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 1024,
        };
        let document = roxmltree::Document::parse_with_options(xml, opt)
            .map_err(|e| format!("Error parsing XML: {}", e))?;

        let mut sender_id: Option<&str> = None;
        let mut urn: Option<&str> = None;
        let mut freq: Option<&str> = None;
        let mut currency: Option<&str> = None;
        let mut currency_denom: Option<&str> = None;
        let mut title: Option<&str> = None;
        let mut unit: Option<&str> = None;
        let mut date: Option<&str> = None;
        let mut rate: Option<&str> = None;

        for node in document.descendants() {
            if node.is_element() {
                match node.tag_name().name() {
                    "Sender" => sender_id = node.attribute("id"),
                    "URN" => urn = node.text(),
                    "Value" => match node.attribute("id") {
                        Some("FREQ") => freq = node.attribute("value"),
                        Some("CURRENCY") => currency = node.attribute("value"),
                        Some("CURRENCY_DENOM") => currency_denom = node.attribute("value"),
                        Some("TITLE") => title = node.attribute("value"),
                        Some("UNIT") => unit = node.attribute("value"),
                        _ => {}
                    },
                    "Obs" => {
                        for child in node.children() {
                            match child.tag_name().name() {
                                "ObsDimension" => date = child.attribute("value"),
                                "ObsValue" => rate = child.attribute("value"),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        let ecb_response = EcbResponse {
            sender_id: sender_id.ok_or_else(|| "Sender ID not found")?.to_string(),
            urn: urn
                .ok_or_else(|| "Uniform Respource Name not found")?
                .to_string(),
            freq: freq.ok_or_else(|| "Frequency not found")?.to_string(),
            currency: currency.ok_or_else(|| "Currency not found")?.to_string(),
            currency_denom: currency_denom
                .ok_or_else(|| "Currency Denominator not found")?
                .to_string(),
            title: title.ok_or_else(|| "Title not found")?.to_string(),
            unit: unit.ok_or_else(|| "Unit not found")?.to_string(),
            date: date.ok_or_else(|| "Date not found")?.to_string(),
            rate: rate.ok_or_else(|| "Rate not found")?.to_string(),
        };
        Ok(ecb_response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecb_parse_xml_from_file() {
        let xml_data: &str = include_str!("../data/ecb_example_response.xml");

        let ecb_response = EcbResponse::from_xml_string(xml_data).unwrap();
        assert_eq!(ecb_response.sender_id, "ECB");
        assert_eq!(
            ecb_response.urn,
            "urn:sdmx:org.sdmx.infomodel.datastructure.DataStructure=ECB:ECB_EXR1(1.0)"
        );
        assert_eq!(ecb_response.freq, "D");
        assert_eq!(ecb_response.currency, "USD");
        assert_eq!(ecb_response.currency_denom, "EUR");
        assert_eq!(ecb_response.title, "US dollar/Euro");
        assert_eq!(ecb_response.unit, "USD");
        assert_eq!(ecb_response.date, "2023-07-13");
        assert_eq!(ecb_response.rate, "1.1182");
    }

    #[test]
    fn test_ecb_content_type_from_url() {
        let query = [("startPeriod", "2023-07-13"), ("endPeriod", "2023-07-13")];

        let client = reqwest::blocking::Client::new();
        let res: reqwest::blocking::Response = client
            .get(ECB_URL)
            .query(&query)
            .send()
            .expect("Error while sending request");

        assert_eq!(
            res.headers().get("content-type").unwrap().to_str().unwrap(),
            "application/vnd.sdmx.genericdata+xml;version=2.1"
        );
    }

    #[test]
    fn test_ecb_get_blocking_exchange_rate_from_url() {
        let query = [("startPeriod", "2023-07-13"), ("endPeriod", "2023-07-13")];
        let response: String =
            get_blocking_exchange_rate(ECB_URL, &query).expect("Failed to get exchange rate");
        println!("{}", response);
        assert!(response.len() > 0);
    }

    #[test]
    fn test_ecb_parse_exchange_rate_from_url() {
        // thursday
        {
            let date = "2023-07-13";
            let query = [("startPeriod", date), ("endPeriod", date)];
            let response: String =
                get_blocking_exchange_rate(ECB_URL, &query).expect("Failed to get exchange rate");
            let ecb_response = EcbResponse::from_xml_string(&response).unwrap();

            assert_eq!(ecb_response.freq, "D");
            assert_eq!(ecb_response.currency, "USD");
            assert_eq!(ecb_response.currency_denom, "EUR");
            assert_eq!(ecb_response.title, "US dollar/Euro");
            assert_eq!(ecb_response.unit, "USD");
            assert_eq!(ecb_response.date, "2023-07-13");
            assert_eq!(ecb_response.rate, "1.1182");
        }
        // sunday fails
        {
            let date = "2024-09-28";
            let query = [("startPeriod", date), ("endPeriod", date)];
            let response =
                get_blocking_exchange_rate(ECB_URL, &query).expect("Failed to get exchange rate");
            let ecb_response = EcbResponse::from_xml_string(&response);
            assert_eq!(ecb_response.is_err(), true);
        }
        // future fails
        {
            let date = chrono::Local::now() + chrono::Duration::days(2);
            let date_str = date.format("%Y-%m-%d").to_string();
            let query = [("startPeriod", &date_str), ("endPeriod", &date_str)];
            let response =
                get_blocking_exchange_rate(ECB_URL, &query).expect("Failed to get exchange rate");
            let ecb_response = EcbResponse::from_xml_string(&response);
            assert_eq!(ecb_response.is_err(), true);
        }
    }

    #[test]
    fn test_ecb_inverse_currency_exchange_rate_from_file() {
        let xml_data: &str = include_str!("../data/ecb_example_response.xml");

        let ecb_response = EcbResponse::from_xml_string(xml_data).unwrap();
        let rate: f32 = ecb_response.rate.parse().unwrap();
        let inverse_rate: f32 = invert_exchange_rate(rate).unwrap();
        assert_eq!(inverse_rate, 1.0 / 1.1182);
    }

    #[test]
    fn test_ecb_url_content_type() {
        let query = [("startPeriod", "2023-07-13"), ("endPeriod", "2023-07-13")];

        let client = reqwest::blocking::Client::new();
        let res = client
            .get(ECB_URL)
            .query(&query)
            .send()
            .expect("Error while sending request");

        assert_eq!(
            res.headers().get("content-type").unwrap().to_str().unwrap(),
            "application/vnd.sdmx.genericdata+xml;version=2.1"
        );
    }

    #[test]
    fn test_ecb_url_get_exchange_rate() {
        let query = [("startPeriod", "2023-07-13"), ("endPeriod", "2023-07-13")];
        let response: String =
            get_blocking_exchange_rate(ECB_URL, &query).expect("Failed to get exchange rate");
        println!("{}", response);
        assert!(response.len() > 0);
    }
}
