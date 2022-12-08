# Rust interceptor model

## 通用拦截器

每次调用都会产生所有管道拦截器的拷贝动作 相当于所有状态不能在拦截器中保存

## 函数式存储模型

贯穿所有拦截器

只能在context中进行存储 旨在除了context状态之外 所有拦截器幂等

## 高阶设计模型
add => 1 2 3
Z(next) => Y(context) => X(next, context)

 ⬇
 
Z1(Z2(Z3(next)))(context)

