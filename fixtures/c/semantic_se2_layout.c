/* Independent GCC layout oracle. Record bytes at the final printf stop. */
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

struct Plain { char flag; int count; short tail; };
struct Packed { char flag; int count; } __attribute__((packed));
union Overlay { uint32_t word; unsigned char bytes[4]; };
struct Bits { unsigned a:3; signed b:5; unsigned c:9; };
struct Node { int value; struct Node *next; };
struct Nested { struct Plain part; struct Node node; };
struct Flex { int length; char tail[]; };
enum Shade { DARK=-2, LIGHT=3, BRIGHT=3 };

int main(void) {
    struct Plain sample={1,0x01020304,-2};
    struct Packed packed={2,0x10203040};
    union Overlay overlay={.word=0x11223344u};
    struct Bits bits={5,-3,257};
    struct Node node={19,0};
    node.next=&node;
    struct Nested nested={{5,27,6},{17,0}};
    nested.node.next=&node;
    struct Plain rows[2]={{3,7,8},{4,9,10}};
    int matrix[2][3]={{1,2,3},{4,5,6}};
    int *pointers[2]={&sample.count,&rows[1].count};
    char letters[4]={'A','B','C','\0'};
    _Bool flag=1;
    enum Shade shade=DARK;
    float f=1.25f;
    double d=-2.5;
    int n=3;
    int dynamic[n];
    for(int i=0;i<n;++i) dynamic[i]=i+20;
    printf("ORACLE plain=%zu,%zu,%zu,%zu packed=%zu,%zu,%zu bits=%zu node=%zu,%zu nested=%zu,%zu matrix=%zu rows=%zu\n",
        sizeof(sample),offsetof(struct Plain,count),offsetof(struct Plain,tail),sizeof(struct Plain),
        sizeof(packed),offsetof(struct Packed,count),sizeof(struct Packed),sizeof(bits),
        sizeof(node),offsetof(struct Node,next),sizeof(nested),offsetof(struct Nested,node),sizeof(matrix),sizeof(rows));
    return sample.count!=0x01020304 || packed.count!=0x10203040 ||
           overlay.word!=0x11223344u || bits.a!=5 || bits.b!=-3 || bits.c!=257 ||
           node.next!=&node || nested.part.count!=27 || nested.node.value!=17 ||
           matrix[1][2]!=6 || rows[1].count!=9 ||
           pointers[0]!=&sample.count || letters[0]!='A' || !flag || shade!=DARK ||
           f!=1.25f || d!=-2.5 || dynamic[2]!=22;
}
