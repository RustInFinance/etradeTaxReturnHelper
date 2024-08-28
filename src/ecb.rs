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
        println!("{:?}", document);

        for node in document.descendants() {
            if node.is_element() {
                println!("{:?}", node.tag_name());
            }
        }
    }
}
