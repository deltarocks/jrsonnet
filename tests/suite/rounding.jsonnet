std.assertEqual("%.0g" % 1.0, "1") &&
std.assertEqual("%.0g" % 0.0, "0") &&
std.assertEqual("%.0g" % 0.1, "0.1") &&
std.assertEqual("%#.0g" % 1.0, "1.") &&
std.assertEqual("%.1g" % 9.9, "1e+01") &&
std.assertEqual("%.2g" % 99.9, "1e+02") &&
std.assertEqual("%.0e" % 9.5e10, "1e+11") &&
std.assertEqual("%.1e" % 9.99, "1.0e+01") &&
std.assertEqual("%.0g" % 0.00012, "0.0001") &&
true
