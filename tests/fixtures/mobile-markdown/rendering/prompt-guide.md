## Prompting: less effective vs better

| # | Less effective ❌ | Better ✅ |
| :-: | --- | --- |
| 1 | Summarize the text below as a bullet point list of the most important points. {text input here} | Summarize the text below as a bullet point list of the most important points. Text: `"""{text input here}"""` |
| 2 | Write a poem about OpenAI. | Write a short inspiring poem about OpenAI, focusing on the recent DALL-E product launch (DALL-E is a text to image ML model) in the style of a {famous poet}. |
| 3 | Extract the entities mentioned in the text below. | Extract the important entities mentioned in the text below. First extract all company names, then all people names. Desired format: `Company names: <comma_separated_list>` |

Put instructions at the **beginning** of the prompt and use `###` or `"""` to separate the instruction and context:

```
Summarize the text below as a bullet point list.

Text: """
{text input here}
"""
```

*Guide by Olga Vetlina.*
