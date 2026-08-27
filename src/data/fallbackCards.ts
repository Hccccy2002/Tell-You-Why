import type { KnowledgeCard } from "../types";

const createdAt = "2026-08-26T00:00:00Z";

export const fallbackCards: KnowledgeCard[] = [
  {
    id: "demo-science-ice",
    schemaVersion: 1,
    language: "zh-CN",
    topicId: "natural_science",
    topicLabel: "自然科学",
    tags: ["水", "密度", "晶体结构"],
    question: "为什么大多数物质凝固会收缩，水结冰却会膨胀？",
    shortAnswer:
      "水分子结冰时会被氢键排列成较疏松的晶体结构，分子之间留下更多空隙，因此同样质量的冰体积更大、密度更低。",
    explanation:
      "液态水里的氢键不断断裂和重组，分子能够相对紧密地挤在一起。温度降到冰点附近后，分子运动变慢，氢键把水分子固定成具有规则空隙的六角结构。这个结构占据的体积比液态排列更大，所以水结冰会膨胀，冰也会浮在水面上。水的密度还会随温度变化，在接近 4 摄氏度时较大；继续降温后，开放结构逐步占优势。实际结冰过程还会受到溶质、压力和成核条件影响。这个演示说明来自常见基础科学资料，但尚未经过本项目的人工内容审核流程。",
    whyItMatters: "这种反常性质会影响岩石风化、湖泊生态和冬季水管防护。",
    difficulty: "beginner",
    estimatedReadSeconds: 55,
    sourceRefs: [
      {
        title: "Water Density",
        publisher: "USGS Water Science School",
        url: "https://www.usgs.gov/special-topics/water-science-school/science/water-density",
      },
    ],
    trustStatus: "demo_unreviewed",
    contentFingerprint: "demo-science-ice-v1",
    createdAt,
    isFavorite: false,
  },
  {
    id: "demo-space-sky",
    schemaVersion: 1,
    language: "zh-CN",
    topicId: "space_earth",
    topicLabel: "宇宙与地球",
    tags: ["天空", "光", "散射"],
    question: "为什么晴朗白天的天空通常是蓝色，而日落附近偏红？",
    shortAnswer:
      "大气分子更容易散射波长较短的蓝光；日落时阳光穿过更长的大气路径，蓝光大量散开，直达眼睛的光便更偏红。",
    explanation:
      "太阳光包含多种波长。光进入大气后，与尺寸远小于光波长的分子相互作用，发生瑞利散射，其强度对短波长尤其明显。白天从各方向散射到眼睛的蓝光较多，所以天空显蓝。日出日落时，光线斜穿大气，路径显著变长，蓝紫光在到达观察者前已被散射到其他方向，留下较多橙红光。紫光的散射通常更强，但太阳光谱、人眼敏感度以及高层大气吸收共同影响最终观感，因此天空往往呈蓝色而非紫色。云和气溶胶也会改变颜色。",
    difficulty: "beginner",
    estimatedReadSeconds: 50,
    sourceRefs: [
      {
        title: "Why Is the Sky Blue?",
        publisher: "NASA Space Place",
        url: "https://spaceplace.nasa.gov/blue-sky/en/",
      },
    ],
    trustStatus: "demo_unreviewed",
    contentFingerprint: "demo-space-sky-v1",
    createdAt,
    isFavorite: false,
  },
  {
    id: "demo-history-purple",
    schemaVersion: 1,
    language: "zh-CN",
    topicId: "history_civilization",
    topicLabel: "历史与文明",
    tags: ["颜色", "染料", "贸易"],
    question: "为什么紫色在许多古代社会常常与权力和地位联系在一起？",
    shortAnswer:
      "一些古代紫色染料需要从大量海螺中少量提取，制作费时且价格极高，能长期使用这种颜色便成了财富和身份的信号。",
    explanation:
      "著名的泰尔紫来自骨螺分泌物。原料采集、处理和染色都很繁琐，而且少量染料需要许多贝类。稀缺性让紫色纺织品在地中海世界价格高昂，并被统治者和精英采用。昂贵颜色还可能受到服饰规范和宫廷制度限制，进一步强化其身份含义。后来合成染料降低成本后，紫色不再天然代表稀缺，但旧有象征仍留在一些文化表达中。不同社会对颜色的象征并不完全一致，因此不能把这种联系当作跨文化、跨时代的统一规则。",
    difficulty: "general",
    estimatedReadSeconds: 50,
    sourceRefs: [
      {
        title: "Tyrian purple",
        publisher: "Encyclopaedia Britannica",
        url: "https://www.britannica.com/technology/Tyrian-purple",
      },
    ],
    trustStatus: "demo_unreviewed",
    contentFingerprint: "demo-history-purple-v1",
    createdAt,
    isFavorite: false,
  },
  {
    id: "demo-language-week",
    schemaVersion: 1,
    language: "zh-CN",
    topicId: "language_writing",
    topicLabel: "语言与文字",
    tags: ["星期", "语言", "命名"],
    question: "为什么中文把一周中的日子叫“星期”，英语却常用天体命名？",
    shortAnswer:
      "不同语言继承了不同的历法和命名传统：现代汉语的“星期”系统强调顺序，英语星期名则混合了罗马天体与日耳曼神祇名称。",
    explanation:
      "英语 Sunday、Monday 与太阳和月亮有关，其余若干名称把罗马神祇对应为日耳曼传统中的神祇。汉语历史上也曾使用与七曜相关的名称，现代常用的“星期一”到“星期六”则以数字排序，“星期日”保留日的称呼。其他语言也可能采用数字、市场日或宗教传统命名，同一套七日周期在传播时会被本地语言重新解释。词源说明名称的来历，并不规定当代人的信仰或习惯。名称记录了文化接触和历法传播，但今天的使用者通常并不会意识到这些词源。",
    difficulty: "general",
    estimatedReadSeconds: 55,
    sourceRefs: [],
    trustStatus: "demo_unreviewed",
    contentFingerprint: "demo-language-week-v1",
    createdAt,
    isFavorite: false,
  },
  {
    id: "demo-computer-cache",
    schemaVersion: 1,
    language: "zh-CN",
    topicId: "computing_internet",
    topicLabel: "计算机与互联网",
    tags: ["缓存", "性能", "局部性"],
    question: "为什么电脑需要多级缓存，而不只使用容量更大的内存？",
    shortAnswer:
      "处理器速度远高于主内存访问速度。小而快的缓存把近期或相邻数据放得更靠近处理器，用容量换取等待时间的减少。",
    explanation:
      "存储器通常在速度、容量和成本之间取舍。处理器访问寄存器和一级缓存很快，但它们昂贵且容量小；主内存容量大，却需要更多等待周期。程序又常表现出时间局部性和空间局部性，也就是刚访问过或附近的数据很可能再次使用。多级缓存利用这种规律，让大部分访问落在更快的层级，同时保留较大的总体容量。如果数据没有命中某一级缓存，请求才继续到更慢、更大的下一级。缓存也会带来一致性和替换策略等复杂问题，所以层级数量并非越多越好。",
    difficulty: "general",
    estimatedReadSeconds: 55,
    sourceRefs: [],
    trustStatus: "demo_unreviewed",
    contentFingerprint: "demo-computer-cache-v1",
    createdAt,
    isFavorite: false,
  },
  {
    id: "demo-business-compound",
    schemaVersion: 1,
    language: "zh-CN",
    topicId: "business_economics",
    topicLabel: "商业与经济常识",
    tags: ["复利", "增长率", "时间"],
    question: "为什么相同的年增长率，时间越长越不能用简单加法估算？",
    shortAnswer:
      "每一期的增长会进入下一期的计算基数，增长发生在已经增长过的数值上，因此长期结果是连乘而不是把百分比简单相加。",
    explanation:
      "如果一个数每年增长 10%，第一年后变成原来的 1.1 倍，第二年则是在新基数上再乘 1.1，结果为 1.21 倍，而不是 1.2 倍。时间越长，连乘与简单相加的差距越明显。同样的规律也适用于人口、库存等按比例变化的数量。估算长期结果时，要明确增长率是否固定、计算周期如何定义，以及中间是否有新增或取出。复利只是数学上的累积效应；现实中的收益率会波动，也可能为负，不能把固定示例理解为投资承诺。",
    difficulty: "beginner",
    estimatedReadSeconds: 50,
    sourceRefs: [],
    trustStatus: "demo_unreviewed",
    contentFingerprint: "demo-business-compound-v1",
    createdAt,
    isFavorite: false,
  },
  {
    id: "demo-life-onion",
    schemaVersion: 1,
    language: "zh-CN",
    topicId: "daily_principles",
    topicLabel: "日常生活原理",
    tags: ["洋葱", "化学", "眼泪"],
    question: "切洋葱时眼睛为什么会流泪，冷藏后再切常会好一些？",
    shortAnswer:
      "洋葱细胞被切开后会产生挥发性刺激物，接触眼表会触发泪液保护；低温可让相关反应和挥发速度减慢。",
    explanation:
      "刀切破洋葱细胞后，原本分隔的酶和含硫化合物相遇，经过反应形成易挥发的催泪因子。它到达眼表后造成刺激，泪腺便分泌泪液来稀释和冲走刺激物。短时间冷藏通常会降低反应与挥发速度，但不会完全消除；使用锋利刀具减少细胞挤压、保持通风也可能有所帮助。刺激强弱还取决于洋葱品种、新鲜度、切法和空气流动。冷藏只是一种降低挥发的办法，操作时仍应注意刀具安全，不要为了避泪采用危险姿势。",
    difficulty: "beginner",
    estimatedReadSeconds: 50,
    sourceRefs: [],
    trustStatus: "demo_unreviewed",
    contentFingerprint: "demo-life-onion-v1",
    createdAt,
    isFavorite: false,
  },
  {
    id: "demo-art-perspective",
    schemaVersion: 1,
    language: "zh-CN",
    topicId: "arts_culture",
    topicLabel: "艺术与文化",
    tags: ["绘画", "透视", "视觉"],
    question: "为什么线性透视能让平面的画布产生明显的空间深度感？",
    shortAnswer:
      "它把平行方向的线条按视觉投影规律汇聚到消失点，并让远处物体显得更小，从而模拟眼睛观察三维空间时的几何线索。",
    explanation:
      "当我们看向远方，道路两侧等现实中的平行线会在视野中显得逐渐接近。线性透视把这种投影关系系统化：设定视平线和一个或多个消失点，再按距离缩小物体尺度。大脑熟悉这些线索，因此会把画布上的二维形状解释为有前后距离的空间。不过，透视只是许多深度线索之一，遮挡、明暗和空气透视也很重要。不同文化和时期也发展出不以单点透视为核心的空间表现方式。线性透视不是更“正确”的绘画方法，而是一套适合特定视觉效果的约定和工具。",
    difficulty: "general",
    estimatedReadSeconds: 55,
    sourceRefs: [],
    trustStatus: "demo_unreviewed",
    contentFingerprint: "demo-art-perspective-v1",
    createdAt,
    isFavorite: false,
  },
];

export const presetTopics = [
  ["natural_science", "自然科学"],
  ["space_earth", "宇宙与地球"],
  ["history_civilization", "历史与文明"],
  ["language_writing", "语言与文字"],
  ["computing_internet", "计算机与互联网"],
  ["business_economics", "商业与经济常识"],
  ["daily_principles", "日常生活原理"],
  ["arts_culture", "艺术与文化"],
] as const;
