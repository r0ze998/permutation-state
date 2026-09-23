F=[0]+[round(n**0.2*1000) for n in range(1,65)]
K=5500
def dmg(nA,strA,strB,nB,mods=(),v=10000):
    n=max(1,min(64,(nA+nB)//1000))
    x=nA*strA*K//10000//strB
    x=x*1000//F[n]
    for m in mods: x=x*m//10000
    x=x*v//10000
    return x
def show(name,a,b):
    print(name,'A->B',a,'B->A',b)
# a) 10 spear vs 10 spear, plains, v=1
show('a',dmg(10000,10,10,10000),dmg(10000,10,10,10000))
# b) 20 spear attack 10 horsemen (spear counter 15000); horse retaliates
show('b',dmg(20000,10,10,10000,(15000,)),dmg(10000,10,10,20000))
# c) 10 archers ranged at 12 spear on hills (ranged 8000, terrain 8000); no retaliation
show('c',dmg(10000,10,10,12000,(15000,8000,8000)),0)
# d) 20 spear vs city pop10 walls: city virtual troops 4+10=14 (14000), str 10, walls dmg-to-city 6667; city retaliation 5000
show('d',dmg(20000,10,10,14000,(6667,)),dmg(14000,10,10,20000,(5000,)))
# e) 10 pikemen(T2 22) vs 20 spear
show('e',dmg(10000,22,10,20000),dmg(20000,10,22,10000))
# f) variance extremes on (a)
print('f min/max', dmg(10000,10,10,10000,v=9000), dmg(10000,10,10,10000,v=11000))
print('F table:')
for i in range(1,65,8): print(' ',' '.join(f'{j}:{F[j]}' for j in range(i,i+8)))
