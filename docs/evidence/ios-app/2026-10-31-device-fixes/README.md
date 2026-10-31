# iOS device fixes: changed screens (ADR 0088)

`SurveyUITests` on the iPhone 17 Pro Simulator (iOS 26.1), light and dark,
downscaled to 1000 px high, for lawkitt/mdoc#293.

| Shot | Screen |
| --- | --- |
| 02 | The + menu: Paste, Files, Photos, Scan, Paste nearest the button |
| 31 | Home below the detector floor: no setup card, no promise to find names |
| 32 | A pasted document below the floor: the note where Find names would be, then Share, Copy, Paste reply |
| 33 | Settings below the floor: the same note, and the text-recognition download |

The cellular question, "Waiting for Wi-Fi" and Finish import need a real
network change or an interrupted import; the view-model tests cover them,
and the device checklist (P17) runs them on a phone.
