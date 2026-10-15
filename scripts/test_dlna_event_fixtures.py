"""Independent XML verification of original synthetic DLNA event fixtures."""

import unittest
import xml.etree.ElementTree as ET
from pathlib import Path


class DlnaEventFixtures(unittest.TestCase):
    def test_rendering_control_two_xml_layers(self):
        fixture = (
            Path(__file__).resolve().parents[1]
            / "crates/frameark-dlna/tests/fixtures/rcs-volume-73.xml"
        )
        outer = ET.fromstring(fixture.read_bytes())
        event_ns = "{urn:schemas-upnp-org:event-1-0}"
        self.assertEqual(outer.tag, event_ns + "propertyset")
        self.assertEqual(len(outer), 1)
        self.assertEqual(outer[0].tag, event_ns + "property")
        self.assertEqual(len(outer[0]), 1)
        last_change = outer[0][0]
        self.assertEqual(last_change.tag, "LastChange")
        self.assertEqual(len(last_change), 0)
        inner = ET.fromstring(last_change.text)
        rcs = "{urn:schemas-upnp-org:metadata-1-0/RCS/}"
        self.assertEqual(inner.tag, rcs + "Event")
        self.assertEqual(len(inner), 1)
        self.assertEqual(inner[0].tag, rcs + "InstanceID")
        self.assertEqual(inner[0].attrib, {"val": "0"})
        self.assertEqual(len(inner[0]), 2)
        self.assertEqual(inner[0][0].tag, rcs + "PresetNameList")
        self.assertEqual(inner[0][0].attrib, {"val": "FactoryDefaults"})
        self.assertEqual(inner[0][1].tag, rcs + "Volume")
        self.assertEqual(inner[0][1].attrib, {"channel": "Master", "val": "73"})


if __name__ == "__main__":
    unittest.main()
