def G(p): return 15 + 8*(p-1) + int((p-1)**1.5)
PATH=[40,40,90,90,90,160,160,260,320,400]
STAGE=[200,300,400]
def sim(maxc, troop_target, T=180, verbose=False):
    cities=[{'pop':1,'food':0,'prod':0,'bld':set()}]
    gold=20; sci_store=0; tech_i=0; stage=0; sg_prod=0; troops=0; sci_total=0
    out={}; bankrupt_ticks=0; stage_ticks=[]
    for t in range(T):
        C=len(cities); income=0; scit=0
        order=['granary','workshop','academy','temple','market']
        for idx,c in enumerate(cities):
            p=c['pop']
            f=2+p*2.2+(2 if 'granary' in c['bld'] else 0)
            pr=1+p*0.9+(2 if 'workshop' in c['bld'] else 0)
            g=2+p//2+(3 if 'market' in c['bld'] else 0)
            s=1+p//3+(3 if 'academy' in c['bld'] else 0)
            ame=3-p//3-(C-1)//3+(2 if 'temple' in c['bld'] else 0)
            surplus=f-2*p
            if ame<=-3: surplus=0; pr*=0.75
            elif ame<0: surplus*=0.5
            elif ame>=2: surplus*=1.1
            c['food']+=surplus
            if c['food']>=G(p): c['food']-=G(p); c['pop']+=1
            c['prod']+=pr
            if idx==0 and tech_i>=8+stage and stage<3:   # capital builds star gate when unlocked (Astronomy idx4 -> stage1)
                c['prod']-=pr; sg_prod+=pr
                cost=STAGE[stage]
                if t>=120: cost=cost*max(60,100-(t-120))//100
                if sg_prod>=cost: sg_prod-=cost; stage+=1; stage_ticks.append(t)
            elif C<maxc and p>=2 and c['prod']>=30:
                c['prod']-=30; c['pop']-=1; cities.append({'pop':1,'food':0,'prod':0,'bld':set()})
            else:
                for b,cost in [('granary',40),('workshop',40),('academy',60),('temple',50),('market',60)]:
                    if b not in c['bld']:
                        if c['prod']>=cost: c['prod']-=cost; c['bld'].add(b)
                        break
                else:
                    if troops<troop_target:
                        n=int(min(c['prod']//6, troop_target-troops)); troops+=n; c['prod']-=6*n
            income+=g; scit+=s
        up=troops*(20+troops)/80 + (len(cities)-1)**2/3
        gold+=income-up
        if gold<0: bankrupt_ticks+=1; gold=0
        sci_total+=scit
        mult=1+0.1*(len(cities)-1)
        sci_store+=scit
        while tech_i<len(PATH) and sci_store>=PATH[tech_i]*mult: sci_store-=PATH[tech_i]*mult; tech_i+=1; out.setdefault('tech_done',[]).append(t)
        if t in (30,60,90,120,150,179):
            out[t]=dict(C=len(cities),pop=sum(c['pop'] for c in cities),inc=round(income,1),up=round(up,1),gold=round(gold),sci=scit,troops=troops)
    out['stages']=stage_ticks; out['bankrupt_ticks']=bankrupt_ticks
    return out
for name,mc,tt in [('tall-4 science',4,10),('wide-8 builder',8,20),('wide-8 militar',8,60)]:
    o=sim(mc,tt); print(name)
    for k,v in o.items(): print('  ',k,v)
