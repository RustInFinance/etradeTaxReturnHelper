use roxmltree;

#[cfg(test)]
mod tests {
    #[test]
    fn test_ecb_read_xml() {
        let xml_data: &str = include_str!("../data/ecb_example_response.xml");

        let opt = roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 1024,
        };
        let document =
            roxmltree::Document::parse_with_options(xml_data, opt).expect("Failed to parse");

        let mut sender_id = Option::<&str>::None;
        let mut urn = Option::<&str>::None;
        let mut freq = Option::<&str>::None;
        let mut currency = Option::<&str>::None;
        let mut currency_denom = Option::<&str>::None;
        let mut title = Option::<&str>::None;
        let mut unit = Option::<&str>::None;
        let mut date = Option::<&str>::None;
        let mut rate = Option::<&str>::None;

        for node in document.descendants() {
            if node.is_element() {
                if node.tag_name().name() == "Sender" {
                    sender_id = node.attribute("id");
                }
                if node.tag_name().name() == "URN" {
                    urn = node.text();
                }
                if node.tag_name().name() == "Value" {
                    if node.attribute("id") == Some("FREQ") {
                        freq = node.attribute("value");
                    }
                    if node.attribute("id") == Some("CURRENCY") {
                        currency = node.attribute("value");
                    }
                    if node.attribute("id") == Some("CURRENCY_DENOM") {
                        currency_denom = node.attribute("value");
                    }
                    if node.attribute("id") == Some("TITLE") {
                        title = node.attribute("value");
                    }
                    if node.attribute("id") == Some("UNIT") {
                        unit = node.attribute("value");
                    }
                }
                if node.tag_name().name() == "Obs" {
                    for child in node.children() {
                        if child.tag_name().name() == "ObsDimension" {
                            date = child.attribute("value");
                        }
                        if child.tag_name().name() == "ObsValue" {
                            rate = child.attribute("value");
                        }
                    }
                }
                // This didn't work, therfore the above is used
                // if (node.tag_name().name() == "ObsDimension") {
                //     let date = node.attribute("value");
                // }
                // if (node.tag_name().name() == "ObsValue") {
                //     let rate = node.attribute("value");
                // }
            }
        }

        assert_eq!(sender_id, Some("ECB"));
        assert_eq!(
            urn,
            Some("urn:sdmx:org.sdmx.infomodel.datastructure.DataStructure=ECB:ECB_EXR1(1.0)")
        );
        assert_eq!(freq, Some("D"));
        assert_eq!(currency, Some("USD"));
        assert_eq!(currency_denom, Some("EUR"));
        assert_eq!(title, Some("US dollar/Euro"));
        assert_eq!(unit, Some("USD"));
        assert_eq!(date, Some("2023-07-13"));
        assert_eq!(rate, Some("1.1182"));
    }
}
