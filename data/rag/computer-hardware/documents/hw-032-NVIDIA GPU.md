# NVIDIA GPU

> 资料编号：hw-032｜主题：多核与SoC｜难度：进阶
> 作者：胡伟武、汪文祥、苏孟豪、张福新、王焕东、章隆兵、肖俊华、刘苏、陈新科、吴瑞阳、李晓钰、高燕萍
> 来源：[计算机体系结构基础（第三版）](https://github.com/foxsen/archbase/blob/dfc1ea3ffc22b5f09c51cc3af80b25c3ce6ccd6c/21-multicore.Rmd#L352-L391)
> 许可：[CC-BY-NC-4.0](https://creativecommons.org/licenses/by-nc/4.0/)；原作者署名及许可须随文本保留。
> 固定版本：dfc1ea3ffc22b5f09c51cc3af80b25c3ce6ccd6c；原文第 352–391 行。
> 整理说明：按自然章节节选；去除 R 排版执行块和图片并保留占位说明；规范标题和少量 HTML；将相对链接指回固定版本；不翻译、不用 AI 改写正文。
> 使用范围：原文以历史架构为例，不代表当前 NVIDIA GPU 型号参数或最新架构。
> 本文已核对来源和提取范围，未经逐条事实复核。

---

GPU（Graphics Processing Unit）是进行快速图形处理的硬件单元，现代GPU包括数百个并行浮点运算单元，是典型的众核处理器架构。本节主要介绍NVIDIA公司的Fermi GPU体系结构。

第一个基于Fermi体系结构的GPU芯片有30亿个晶体管，支持512个CUDA核心，组织成16个流多处理器（Stream Multiprocessor，简称SM）。SM结构如图（原文交叉引用：fig:sm-single）、（原文交叉引用：fig:sm-whole）所示。每个SM包含32个CUDA核心（Core）、16个load/store单元（LD/ST）、4个特殊处理单元（Special Function Unit，简称SFU）、64KB的片上高速存储。每个CUDA核心支持一个全流水的定点算术逻辑单元（ALU）和浮点单元（FPU）（如图（原文交叉引用：fig:cuda-core）所示），每个时钟周期可以执行一条定点或者浮点指令。ALU支持所有指令的32位精度运算；FPU实现了IEEE 754-2008浮点标准，支持单精度和双精度浮点的融合乘加指令（Fused Multiply-Add, 简称FMA）。16个load/store单元可以每个时钟周期为16个线程计算源地址和目标地址，实现对这些地址数据的读写。SFU支持超越函数的指令，如sin、cos、平方根等。64KB片上高速存储是可配置的，可配成48KB的共享存储和16KB一级Cache或者16KB共享存储和48KB一级Cache。片上共享存储使得同一个线程块的线程之间能进行高效通信，可以减少片外通信以提高性能。

> [原文图表未展开：单个Fermi流多处理器结构图。请查看原文；本块不能作为文字证据。]

> [原文图表未展开：Fermi流多处理器整体结构图。请查看原文；本块不能作为文字证据。]

> [原文图表未展开：CUDA核结构。请查看原文；本块不能作为文字证据。]

1.Fermi的线程调度

Fermi体系结构使用两层分布式线程调度器。块调度器将线程块（Thread Block）调度到SM上， SM以线程组Warp为单位调度执行，每个Warp包含32个并行线程，这些线程以单指令多线程（Single Instruction Multi Thread,简称SIMT）的方式执行。SIMT类似于SIMD，表示指令相同但处理的数据不同。每个SM有两个Warp调度器和两个指令分派单元，允许两个Warp被同时发射和并发执行。双Warp调度器（Dual Warp Scheduler）选择两个Warp，从每个Warp中发射一条指令到一个16个核构成的组、16个load/store单元，或者4个SFU单元。大多数指令是能够双发射的，例如两条定点指令、两条浮点指令，或者是定点、浮点、load、store、SPU指令的混合。双精度浮点指令不支持与其他指令的双发射。

2.Fermi存储层次

Fermi体系结构的存储层次由每个SM的寄存器堆、每个SM的一级Cache、统一的二级Cache和全局存储组成。图（原文交叉引用：fig:fermi-mem-hierarchy）为Fermi存储层次示意图。具体如下:

1）寄存器。每个SM有32K个32位寄存器，每个线程可以访问自己的私有寄存器,随线程数目的不同，每个线程可访问的私有寄存器数目在21~63间变化。

2）一级Cache和共享存储。每个SM有片上高速存储，主要用来缓存单线程的数据或者用于多线程间的共享数据，可以在一级Cache和共享存储之间进行配置。

3）L2 Cache。768KB统一的二级Cache在16个SM间共享，服务于所有到全局内存中的load/store操作。

4）全局存储。所有线程共享的片外存储。

Fermi体系结构采用CUDA编程环境，可以采用类C语言开发应用程序。NVIDIA将所有形式的并行都定义为CUDA线程，将这种最底层的并行作为编程原语，编译器和硬件可以在GPU上将上千个CUDA线程聚集起来并行执行。这些线程被组织成线程块，以32个为一组（Warp）来执行。Fermi体系结构可以看作GPU与CPU融合的架构，具有强大的浮点计算能力，除了用于图像处理外，也可作为加速器用于高性能计算领域。采用Fermi体系结构的GeForce GTX 480包含480核，主频700MHz，单精度浮点峰值性能为1.536TFLOPS，访存带宽为177.4GB/s。

> [原文图表未展开：Fermi的存储层次图。请查看原文；本块不能作为文字证据。]
