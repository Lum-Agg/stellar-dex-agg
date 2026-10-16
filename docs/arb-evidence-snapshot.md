# Arb operator evidence snapshot

Generated: 2026-10-16T17:09Z · unit: `lumagg-arb` · since: `2026-07-13 00:00:00`

**Confirmed SUCCESS count:** 4858
**Confirmed FAILED count:** 5026

Counts come from the analytics indexer via `/api/v1/arbitrage/stats`;
the journal below is used only for recent hashes and operator log samples.

## Recent SUCCESS hashes

- https://stellar.expert/explorer/public/tx/6fd1211892a44f15d205d6d7f7e085e235544c64f095318c56a40ade8b339c75
- https://stellar.expert/explorer/public/tx/bfe77edec803892f893ed39b228a8f3489c78dd9c06f1a31a1377333c9dd388e
- https://stellar.expert/explorer/public/tx/ca68d9fa3512bd75a1afd0768f7aa7ccacd821d6d682b746a1bfe222e564e57b
- https://stellar.expert/explorer/public/tx/bcd38481f88450e7f976a8b8b32456e2ec68cc97bcb6822113a52f5091109e36
- https://stellar.expert/explorer/public/tx/916e4324f5bc5c77d9ad749877ccdbbd719de194b17c24bfa51d9d2311662bc4
- https://stellar.expert/explorer/public/tx/811a626921f81a56f8cb9c33250acbd3cc1c6a311d577f2c81ff1ae0aa17e023
- https://stellar.expert/explorer/public/tx/0cacacb3dd66571305da11ccba8cdb804c0ed04ab2531b5371a0b0a0dfcafb8a
- https://stellar.expert/explorer/public/tx/3c632eea7c52e501af2568a6fb170a0af14c5e3015add965038ba86df5497815
- https://stellar.expert/explorer/public/tx/9a80c589799cdafcebeddb0f1b9a7c8d3d4fc28e2b5cdeabdfd2cecfb98acc77
- https://stellar.expert/explorer/public/tx/2ed2946e64294ff787caa58ceb520262bac55be1751965b186e095b1ec7e9ed8
- https://stellar.expert/explorer/public/tx/0df041114e51e15d6383a48cd48ad52f229b903805f7b2ac11975faa35fd1c67
- https://stellar.expert/explorer/public/tx/e137f3af43d2064eaaea5a2b5c9eb46390d7e34c6f408176fa474a378113bb75
- https://stellar.expert/explorer/public/tx/b5517d1b554740913378f510035316eb7f49151b784c8db2e28dc20d80cefae3
- https://stellar.expert/explorer/public/tx/8d6916ae17d2d5be29e27e45d8302926f1312286cce176977381b4e47ff3065f

## Sample log lines
```
Oct 16 13:27:55 hzfra-lumagg lumagg-arbitrage-bot[3131721]: 2026-10-16T11:27:55.948928Z  INFO arbitrage::submit: arb tx SUCCESS hash=2ed2946e64294ff787caa58ceb520262bac55be1751965b186e095b1ec7e9ed8 route=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA → CDFZUVS5YNLXU7VENKOUDEOHCJGKQNVUBWD7KMN6E7ZROKPYPFLRUJFG → CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA gross_profit=269781 estimated_fee=160395
Oct 16 14:49:01 hzfra-lumagg lumagg-arbitrage-bot[3131721]: 2026-10-16T12:49:01.013855Z  INFO arbitrage::submit: arb tx SUCCESS hash=0df041114e51e15d6383a48cd48ad52f229b903805f7b2ac11975faa35fd1c67 route=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA → CDFZUVS5YNLXU7VENKOUDEOHCJGKQNVUBWD7KMN6E7ZROKPYPFLRUJFG → CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA gross_profit=285322 estimated_fee=183129
Oct 16 17:06:19 hzfra-lumagg lumagg-arbitrage-bot[3131721]: 2026-10-16T15:06:19.576490Z  INFO arbitrage::submit: arb tx SUCCESS hash=e137f3af43d2064eaaea5a2b5c9eb46390d7e34c6f408176fa474a378113bb75 route=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75 → CAESLMGW5LYTIEJI7FJHK6SFSWRELLNVX5Q4WR4UZEALMTRWQDBKDPAG → CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75 gross_profit=78549 estimated_fee=187219
Oct 16 17:22:31 hzfra-lumagg lumagg-arbitrage-bot[3131721]: 2026-10-16T15:22:31.248809Z  INFO arbitrage::submit: arb tx SUCCESS hash=b5517d1b554740913378f510035316eb7f49151b784c8db2e28dc20d80cefae3 route=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA → CDFZUVS5YNLXU7VENKOUDEOHCJGKQNVUBWD7KMN6E7ZROKPYPFLRUJFG → CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA gross_profit=253120 estimated_fee=154972
Oct 16 18:51:44 hzfra-lumagg lumagg-arbitrage-bot[3131721]: 2026-10-16T16:51:44.702753Z  INFO arbitrage::submit: arb tx SUCCESS hash=8d6916ae17d2d5be29e27e45d8302926f1312286cce176977381b4e47ff3065f route=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA → CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV → CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA gross_profit=309857 estimated_fee=191636
```

## Vault / aggregator

| | |
|--|--|
| Vault | `CCQQ3LRFCSGOYSSD6S4MGH6RWWYVDHYPJO6KYDJYC2IDZK4OGCK6P6KN` |
| Aggregator | `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K` |
| Runbook | [arb-operator.md](./arb-operator.md) |
