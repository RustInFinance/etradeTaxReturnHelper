use std::collections::HashMap;

use etradeTaxReturnHelper::Exchange;

pub fn get_exchange_rates() -> HashMap<Exchange, f64> {
   let mut exchange_rates = HashMap::new();
  exchange_rates.insert(Exchange::USD("2024-08-13".to_string()), 3.9314);
  exchange_rates.insert(Exchange::USD("2024-09-11".to_string()), 3.8816);
  exchange_rates.insert(Exchange::USD("2024-08-29".to_string()), 3.867);
  exchange_rates.insert(Exchange::USD("2024-02-14".to_string()), 4.0593);
  exchange_rates.insert(Exchange::USD("2024-10-22".to_string()), 3.9862);
  exchange_rates.insert(Exchange::USD("2024-04-26".to_string()), 4.0245);
  exchange_rates.insert(Exchange::USD("2024-09-26".to_string()), 3.8294);
  exchange_rates.insert(Exchange::USD("2024-01-16".to_string()), 4.0358);
  exchange_rates.insert(Exchange::USD("2024-08-22".to_string()), 3.8456);
  exchange_rates.insert(Exchange::USD("2024-03-25".to_string()), 3.9833);
  exchange_rates.insert(Exchange::USD("2024-07-22".to_string()), 3.9307);
  exchange_rates.insert(Exchange::USD("2024-03-21".to_string()), 3.9431);
  exchange_rates.insert(Exchange::USD("2024-02-26".to_string()), 3.9776);
  exchange_rates.insert(Exchange::USD("2024-06-13".to_string()), 4.0119);
  exchange_rates.insert(Exchange::USD("2024-08-09".to_string()), 3.9604);
  exchange_rates.insert(Exchange::USD("2024-05-28".to_string()), 3.9183);
  exchange_rates.insert(Exchange::USD("2024-09-10".to_string()), 3.8798);
  exchange_rates.insert(Exchange::USD("2024-02-27".to_string()), 3.9682);
  exchange_rates.insert(Exchange::USD("2024-08-23".to_string()), 3.8453);
  exchange_rates.insert(Exchange::USD("2024-01-23".to_string()), 4.0133);
  exchange_rates.insert(Exchange::USD("2024-05-08".to_string()), 4.0202);
  exchange_rates.insert(Exchange::USD("2024-01-24".to_string()), 4.0131);
  exchange_rates.insert(Exchange::USD("2024-02-01".to_string()), 4.0047);
  exchange_rates.insert(Exchange::USD("2024-07-02".to_string()), 4.0375);
  exchange_rates.insert(Exchange::USD("2024-01-05".to_string()), 3.985);
  exchange_rates.insert(Exchange::USD("2024-08-05".to_string()), 3.9331);
  exchange_rates.insert(Exchange::USD("2024-10-03".to_string()), 3.8951);
  exchange_rates.insert(Exchange::USD("2024-02-07".to_string()), 4.0362);
  exchange_rates.insert(Exchange::USD("2024-02-02".to_string()), 3.9641);
  exchange_rates.insert(Exchange::USD("2024-06-28".to_string()), 4.032);
  exchange_rates.insert(Exchange::USD("2024-08-12".to_string()), 3.9488);
  exchange_rates.insert(Exchange::USD("2024-02-06".to_string()), 4.0484);
  exchange_rates.insert(Exchange::USD("2024-01-26".to_string()), 4.0393);
  exchange_rates.insert(Exchange::USD("2024-10-07".to_string()), 3.9368);
  exchange_rates.insert(Exchange::USD("2024-04-04".to_string()), 3.9515);
  exchange_rates.insert(Exchange::USD("2024-10-29".to_string()), 4.0251);
  exchange_rates.insert(Exchange::USD("2024-03-26".to_string()), 3.9704);
  exchange_rates.insert(Exchange::USD("2024-09-25".to_string()), 3.8117);
  exchange_rates.insert(Exchange::USD("2024-10-31".to_string()), 4.0059);
  exchange_rates.insert(Exchange::USD("2024-04-08".to_string()), 3.9546);
  exchange_rates.insert(Exchange::USD("2024-04-22".to_string()), 4.054);
  exchange_rates.insert(Exchange::USD("2024-07-03".to_string()), 3.999);
  exchange_rates.insert(Exchange::USD("2024-05-16".to_string()), 3.9195);
  exchange_rates.insert(Exchange::USD("2024-07-18".to_string()), 3.9296);
  exchange_rates.insert(Exchange::USD("2024-05-15".to_string()), 3.9368);
  exchange_rates.insert(Exchange::USD("2024-07-31".to_string()), 3.9689);
  exchange_rates.insert(Exchange::USD("2024-08-14".to_string()), 3.8963);
  exchange_rates.insert(Exchange::USD("2024-09-30".to_string()), 3.8193);
  exchange_rates.insert(Exchange::USD("2024-10-15".to_string()), 3.9332);
  exchange_rates.insert(Exchange::USD("2024-06-05".to_string()), 3.9607);
  exchange_rates.insert(Exchange::USD("2024-10-17".to_string()), 3.9786);
  exchange_rates.insert(Exchange::USD("2024-08-27".to_string()), 3.8331);
  exchange_rates.insert(Exchange::USD("2024-04-25".to_string()), 4.0276);
  exchange_rates.insert(Exchange::USD("2024-06-18".to_string()), 4.0549);
  exchange_rates.insert(Exchange::USD("2024-10-23".to_string()), 4.0176);
  exchange_rates.insert(Exchange::USD("2024-02-15".to_string()), 4.0495);
  exchange_rates.insert(Exchange::USD("2024-07-09".to_string()), 3.9391);
  exchange_rates.insert(Exchange::USD("2024-06-03".to_string()), 3.9501);
  exchange_rates.insert(Exchange::USD("2024-09-17".to_string()), 3.8354);
  exchange_rates.insert(Exchange::USD("2024-06-26".to_string()), 4.0291);
  exchange_rates.insert(Exchange::USD("2024-01-22".to_string()), 3.9972);
  exchange_rates.insert(Exchange::USD("2024-06-06".to_string()), 3.953);
  exchange_rates.insert(Exchange::USD("2024-10-21".to_string()), 3.9775);
  exchange_rates.insert(Exchange::USD("2024-01-03".to_string()), 3.9909);
  exchange_rates.insert(Exchange::USD("2024-08-28".to_string()), 3.8539);
  exchange_rates.insert(Exchange::USD("2024-03-27".to_string()), 3.9857);
  exchange_rates.insert(Exchange::USD("2024-07-08".to_string()), 3.947);
  exchange_rates.insert(Exchange::USD("2024-02-23".to_string()), 4.005);
  exchange_rates.insert(Exchange::USD("2024-03-12".to_string()), 3.9162);
  exchange_rates.insert(Exchange::USD("2024-03-29".to_string()), 3.9886);
  exchange_rates.insert(Exchange::USD("2024-10-09".to_string()), 3.9266);
  exchange_rates.insert(Exchange::USD("2024-02-28".to_string()), 3.9922);
  exchange_rates.insert(Exchange::USD("2024-10-08".to_string()), 3.9299);
  exchange_rates.insert(Exchange::USD("2024-06-27".to_string()), 4.0312);
  exchange_rates.insert(Exchange::USD("2024-03-07".to_string()), 3.9485);
  exchange_rates.insert(Exchange::USD("2024-03-14".to_string()), 3.9183);
  exchange_rates.insert(Exchange::USD("2024-04-11".to_string()), 3.9707);
  exchange_rates.insert(Exchange::USD("2024-05-22".to_string()), 3.9243);
  exchange_rates.insert(Exchange::USD("2024-01-31".to_string()), 4.0135);
  exchange_rates.insert(Exchange::USD("2024-01-09".to_string()), 3.9612);
  exchange_rates.insert(Exchange::USD("2024-02-08".to_string()), 4.0292);
  exchange_rates.insert(Exchange::USD("2024-04-30".to_string()), 4.0341);
  exchange_rates.insert(Exchange::USD("2024-03-05".to_string()), 3.9838);
  exchange_rates.insert(Exchange::USD("2024-03-28".to_string()), 4.0081);
  exchange_rates.insert(Exchange::USD("2024-06-11".to_string()), 4.0443);
  exchange_rates.insert(Exchange::USD("2024-02-29".to_string()), 3.9803);
  exchange_rates.insert(Exchange::USD("2024-04-03".to_string()), 3.9843);
  exchange_rates.insert(Exchange::USD("2024-05-09".to_string()), 4.0076);
  exchange_rates.insert(Exchange::USD("2024-05-24".to_string()), 3.9376);
  exchange_rates.insert(Exchange::USD("2024-07-04".to_string()), 3.9784);
  exchange_rates.insert(Exchange::USD("2024-08-01".to_string()), 3.9802);
  exchange_rates.insert(Exchange::USD("2024-05-02".to_string()), 4.0474);
  exchange_rates.insert(Exchange::USD("2024-08-16".to_string()), 3.8914);
  exchange_rates.insert(Exchange::USD("2024-09-02".to_string()), 3.8684);
  exchange_rates.insert(Exchange::USD("2024-03-22".to_string()), 3.9928);
  exchange_rates.insert(Exchange::USD("2024-01-11".to_string()), 3.968);
  exchange_rates.insert(Exchange::USD("2024-04-17".to_string()), 4.0741);
  exchange_rates.insert(Exchange::USD("2024-01-29".to_string()), 4.0326);
  exchange_rates.insert(Exchange::USD("2024-09-04".to_string()), 3.8738);
  exchange_rates.insert(Exchange::USD("2024-01-08".to_string()), 3.9812);
  exchange_rates.insert(Exchange::USD("2024-02-16".to_string()), 4.0325);
  exchange_rates.insert(Exchange::USD("2024-01-25".to_string()), 4.0189);
  exchange_rates.insert(Exchange::USD("2024-09-06".to_string()), 3.8489);
  exchange_rates.insert(Exchange::USD("2024-09-12".to_string()), 3.9025);
  exchange_rates.insert(Exchange::USD("2024-01-02".to_string()), 3.9432);
  exchange_rates.insert(Exchange::USD("2024-09-13".to_string()), 3.8659);
  exchange_rates.insert(Exchange::USD("2024-09-18".to_string()), 3.8358);
  exchange_rates.insert(Exchange::USD("2024-05-06".to_string()), 4.0202);
  exchange_rates.insert(Exchange::USD("2024-10-10".to_string()), 3.9355);
  exchange_rates.insert(Exchange::USD("2024-10-24".to_string()), 4.0168);
  exchange_rates.insert(Exchange::USD("2024-10-28".to_string()), 4.0207);
  exchange_rates.insert(Exchange::USD("2024-07-10".to_string()), 3.9324);
  exchange_rates.insert(Exchange::USD("2024-03-06".to_string()), 3.963);
  exchange_rates.insert(Exchange::USD("2024-09-23".to_string()), 3.8571);
  exchange_rates.insert(Exchange::USD("2024-10-11".to_string()), 3.9204);
  exchange_rates.insert(Exchange::USD("2024-06-10".to_string()), 4.0159);
  exchange_rates.insert(Exchange::USD("2024-06-24".to_string()), 4.0319);
  exchange_rates.insert(Exchange::USD("2024-07-24".to_string()), 3.9498);
  exchange_rates.insert(Exchange::USD("2024-05-17".to_string()), 3.9363);
  exchange_rates.insert(Exchange::USD("2024-02-22".to_string()), 3.9804);
  exchange_rates.insert(Exchange::USD("2024-10-01".to_string()), 3.859);
  exchange_rates.insert(Exchange::USD("2024-10-14".to_string()), 3.9288);
  exchange_rates.insert(Exchange::USD("2024-10-16".to_string()), 3.9468);
  exchange_rates.insert(Exchange::USD("2024-03-01".to_string()), 3.9922);
  exchange_rates.insert(Exchange::USD("2024-01-12".to_string()), 3.9746);
  exchange_rates.insert(Exchange::USD("2024-02-19".to_string()), 4.0269);
  exchange_rates.insert(Exchange::USD("2024-05-13".to_string()), 3.9853);
  exchange_rates.insert(Exchange::USD("2024-08-02".to_string()), 3.9672);
  exchange_rates.insert(Exchange::USD("2024-09-05".to_string()), 3.8487);
  exchange_rates.insert(Exchange::USD("2024-01-19".to_string()), 4.0289);
  exchange_rates.insert(Exchange::USD("2024-04-23".to_string()), 4.061);
  exchange_rates.insert(Exchange::USD("2024-03-11".to_string()), 3.9262);
  exchange_rates.insert(Exchange::USD("2024-04-18".to_string()), 4.0559);
  exchange_rates.insert(Exchange::USD("2024-05-07".to_string()), 4.0056);
  exchange_rates.insert(Exchange::USD("2024-05-14".to_string()), 3.9701);
  exchange_rates.insert(Exchange::USD("2024-04-12".to_string()), 3.9983);
  exchange_rates.insert(Exchange::USD("2024-08-19".to_string()), 3.8682);
  exchange_rates.insert(Exchange::USD("2024-08-30".to_string()), 3.8644);
  exchange_rates.insert(Exchange::USD("2024-04-24".to_string()), 4.0417);
  exchange_rates.insert(Exchange::USD("2024-10-18".to_string()), 3.9718);
  exchange_rates.insert(Exchange::USD("2024-10-25".to_string()), 4.0219);
  exchange_rates.insert(Exchange::USD("2024-04-05".to_string()), 3.9571);
  exchange_rates.insert(Exchange::USD("2024-09-19".to_string()), 3.8249);
  exchange_rates.insert(Exchange::USD("2024-06-07".to_string()), 3.9389);
  exchange_rates.insert(Exchange::USD("2024-05-31".to_string()), 3.9389);
  exchange_rates.insert(Exchange::USD("2024-03-13".to_string()), 3.9269);
  exchange_rates.insert(Exchange::USD("2024-09-03".to_string()), 3.8701);
  exchange_rates.insert(Exchange::USD("2024-08-26".to_string()), 3.8284);
  exchange_rates.insert(Exchange::USD("2024-06-04".to_string()), 3.9448);
  exchange_rates.insert(Exchange::USD("2024-01-18".to_string()), 4.0437);
  exchange_rates.insert(Exchange::USD("2024-04-29".to_string()), 4.0346);
  exchange_rates.insert(Exchange::USD("2024-05-23".to_string()), 3.9394);
  exchange_rates.insert(Exchange::USD("2024-07-26".to_string()), 3.9415);
  exchange_rates.insert(Exchange::USD("2024-07-29".to_string()), 3.9556);
  exchange_rates.insert(Exchange::USD("2024-08-08".to_string()), 3.952);
  exchange_rates.insert(Exchange::USD("2024-10-30".to_string()), 3.9989);
  exchange_rates.insert(Exchange::USD("2024-06-25".to_string()), 3.9975);
  exchange_rates.insert(Exchange::USD("2024-09-27".to_string()), 3.8368);
  exchange_rates.insert(Exchange::USD("2024-04-16".to_string()), 4.0687);
  exchange_rates.insert(Exchange::USD("2024-04-19".to_string()), 4.0688);
  exchange_rates.insert(Exchange::USD("2024-07-12".to_string()), 3.9099);
  exchange_rates.insert(Exchange::USD("2024-03-15".to_string()), 3.9392);
  exchange_rates.insert(Exchange::USD("2024-08-06".to_string()), 3.9467);
  exchange_rates.insert(Exchange::USD("2024-01-17".to_string()), 4.0434);
  exchange_rates.insert(Exchange::USD("2024-07-30".to_string()), 3.9567);
  exchange_rates.insert(Exchange::USD("2024-03-04".to_string()), 3.982);
  exchange_rates.insert(Exchange::USD("2024-02-12".to_string()), 4.0189);
  exchange_rates.insert(Exchange::USD("2024-05-20".to_string()), 3.9149);
  exchange_rates.insert(Exchange::USD("2024-07-19".to_string()), 3.9461);
  exchange_rates.insert(Exchange::USD("2024-04-09".to_string()), 3.9223);
  exchange_rates.insert(Exchange::USD("2024-06-20".to_string()), 4.0345);
  exchange_rates.insert(Exchange::USD("2024-07-23".to_string()), 3.9355);
  exchange_rates.insert(Exchange::USD("2024-10-02".to_string()), 3.8792);
  exchange_rates.insert(Exchange::USD("2024-03-20".to_string()), 3.9895);
  exchange_rates.insert(Exchange::USD("2024-08-07".to_string()), 3.9526);
  exchange_rates.insert(Exchange::USD("2024-05-10".to_string()), 3.9866);
  exchange_rates.insert(Exchange::USD("2024-07-05".to_string()), 3.9581);
  exchange_rates.insert(Exchange::USD("2024-02-09".to_string()), 4.0096);
  exchange_rates.insert(Exchange::USD("2024-02-05".to_string()), 4.0244);
  exchange_rates.insert(Exchange::USD("2024-09-20".to_string()), 3.8317);
  exchange_rates.insert(Exchange::USD("2024-04-15".to_string()), 4.0209);
  exchange_rates.insert(Exchange::USD("2024-05-27".to_string()), 3.9196);
  exchange_rates.insert(Exchange::USD("2024-05-21".to_string()), 3.9175);
  exchange_rates.insert(Exchange::USD("2024-02-20".to_string()), 3.9994);
  exchange_rates.insert(Exchange::USD("2024-03-19".to_string()), 3.9866);
  exchange_rates.insert(Exchange::USD("2024-06-12".to_string()), 4.0342);
  exchange_rates.insert(Exchange::USD("2024-02-13".to_string()), 4.0136);
  exchange_rates.insert(Exchange::USD("2024-09-16".to_string()), 3.8438);
  exchange_rates.insert(Exchange::USD("2024-09-09".to_string()), 3.8758);
  exchange_rates.insert(Exchange::USD("2024-06-19".to_string()), 4.0387);
  exchange_rates.insert(Exchange::USD("2024-01-15".to_string()), 3.9963);
  exchange_rates.insert(Exchange::USD("2024-05-29".to_string()), 3.9244);
  exchange_rates.insert(Exchange::USD("2024-08-20".to_string()), 3.8506);
  exchange_rates.insert(Exchange::USD("2024-08-21".to_string()), 3.8565);
  exchange_rates.insert(Exchange::USD("2024-03-08".to_string()), 3.9392);
  exchange_rates.insert(Exchange::USD("2024-01-10".to_string()), 3.9656);
  exchange_rates.insert(Exchange::USD("2024-01-04".to_string()), 3.9684);
  exchange_rates.insert(Exchange::USD("2024-07-01".to_string()), 3.9915);
  exchange_rates.insert(Exchange::USD("2024-07-25".to_string()), 3.9619);
  exchange_rates.insert(Exchange::USD("2024-07-11".to_string()), 3.9257);
  exchange_rates.insert(Exchange::USD("2024-09-24".to_string()), 3.83);
  exchange_rates.insert(Exchange::USD("2024-10-04".to_string()), 3.9118);
  exchange_rates.insert(Exchange::USD("2024-06-14".to_string()), 4.076);
  exchange_rates.insert(Exchange::USD("2024-07-15".to_string()), 3.896);
  exchange_rates.insert(Exchange::USD("2024-07-17".to_string()), 3.921);
  exchange_rates.insert(Exchange::USD("2024-04-02".to_string()), 4.0009);
  exchange_rates.insert(Exchange::USD("2024-07-16".to_string()), 3.9083);
  exchange_rates.insert(Exchange::USD("2024-01-30".to_string()), 4.0301);
  exchange_rates.insert(Exchange::USD("2024-02-21".to_string()), 3.9966);
  exchange_rates.insert(Exchange::USD("2024-03-18".to_string()), 3.9528);
  exchange_rates.insert(Exchange::USD("2024-06-17".to_string()), 4.0728);
  exchange_rates.insert(Exchange::USD("2024-06-21".to_string()), 4.0527);
  exchange_rates.insert(Exchange::USD("2024-04-10".to_string()), 3.9264);
   exchange_rates
}
