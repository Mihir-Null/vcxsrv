#include "../../xorg-server/hw/xwin/winmanaged.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <X11/Xauth.h>

#define CHECK(x) do { if (!(x)) { fprintf(stderr,"FAIL line %d: %s\n",__LINE__,#x); exit(1); } } while (0)

static winManagedOptions
parse(int count, char **args)
{
    winManagedOptions options = {0};
    int i;
    for (i = 1; i < count; i++) {
        int used = winManagedObserve(&options, count, args, i);
        if (used) i += used - 1;
        else if (!strcmp(args[i], "-auth") || !strcmp(args[i], "-listen") ||
                 !strcmp(args[i], "-nolisten") || !strcmp(args[i], "-vmid")) i++;
    }
    return options;
}

int main(int argc, char **argv)
{
    char *missing[] = {"server", "-wslmanaged"};
    char *valid[] = {"server", "-wslmanaged", "-auth", "test.auth", "-wsldistro", "NixOS"};
    const char *conflicts[][2] = {
        {"-ac", NULL}, {"-listen", "tcp"}, {"-listen", "inet"},
        {"-listen", "inet6"}, {"-nolisten", "winlocal"}, {"-nolisten", "hyperv"},
        {"-nowslvsock", NULL}, {"-noclipboard", NULL}, {"-rootless", NULL},
        {"-fullscreen", NULL}, {"-nodecoration", NULL},
        {"-vmid", "00000000-0000-0000-0000-000000000000"},
        {"-query", "host"}, {"-broadcast", NULL}, {"-displayfd", "5"}
    };
    winManagedOptions options;
    size_t j;
    int order;
    if (argc > 1 && !strcmp(argv[1], "option-values")) {
        unsigned char cookie[16]={0,13,10,26,255,128};
        Xauth auth={FamilyWild,0,NULL,1,"1",18,"MIT-MAGIC-COOKIE-1",16,(char *)cookie};
        FILE *file=fopen("-vmid","wb");
        CHECK(file);CHECK(XauWriteAuth(file,&auth));fclose(file);
        for (order = 0; order < 2; order++) {
            char *distroFirst[] = {"server","-wsldistro","-vmid","-auth","-vmid","-wslmanaged"};
            char *profileFirst[] = {"server","-wslmanaged","-auth","-vmid","-wsldistro","-vmid"};
            char **args = order ? profileFirst : distroFirst;
            options = parse(6,args);
            CHECK(winManagedValidate(&options,0,0)==NULL);
            CHECK(winManagedAuthorityError(options.authority,"1")==NULL);
            CHECK(winVsockLegacyOverrides(options.enabled,6,args)==0);
        }
        remove("-vmid");
        {
            char *args[]={"server","-vmid","guid","-listen","hyperv","-nolisten","hyperv"};
            CHECK(winVsockLegacyOverrides(0,7,args)==
                  (WIN_VSOCK_MANUAL_VM|WIN_VSOCK_MANUAL_LISTEN|WIN_VSOCK_MANUAL_NOLISTEN));
        }
        puts("PASS actual VSOCK startup policy: option-looking managed values, generic overrides retained");
        return 0;
    }
    if (argc > 1 && !strcmp(argv[1], "authority-tail")) {
        unsigned char cookie[16]={0,13,10,26,255,128};
        Xauth auth={FamilyWild,0,NULL,1,"1",18,"MIT-MAGIC-COOKIE-1",16,(char *)cookie};
        unsigned char record[64];
        FILE *file=fopen("managed-fixture.auth","wb");
        size_t length, tail;
        CHECK(file);CHECK(XauWriteAuth(file,&auth));fclose(file);
        file=fopen("managed-fixture.auth","rb");CHECK(file);
        length=fread(record,1,sizeof(record),file);fclose(file);CHECK(length>0);
        for(tail=1;tail<length;tail++) {
            file=fopen("managed-fixture.auth","wb");CHECK(file);
            CHECK(fwrite(record,1,length,file)==length);
            CHECK(fwrite(record,1,tail,file)==tail);fclose(file);
            CHECK(winManagedAuthorityError("managed-fixture.auth","1")!=NULL);
        }
        file=fopen("managed-fixture.auth","wb");CHECK(file);
        CHECK(XauWriteAuth(file,&auth));CHECK(XauWriteAuth(file,&auth));fclose(file);
        CHECK(winManagedAuthorityError("managed-fixture.auth","1")==NULL);
        remove("managed-fixture.auth");
        puts("PASS strict authority framing: every truncated trailing record rejected, clean multi-record EOF accepted");
        return 0;
    }
    options=parse(2,missing);
    CHECK(options.enabled);
    CHECK(winManagedValidate(&options,0,0)!=NULL);
    options=parse(6,valid);
    CHECK(winManagedValidate(&options,0,0)==NULL);
    CHECK(!strcmp(options.distro,"NixOS"));
    CHECK(winManagedValidate(&options,1,0)!=NULL);
    CHECK(winManagedValidate(&options,0,1)!=NULL);
    for(j=0;j<sizeof(conflicts)/sizeof(conflicts[0]);j++) for(order=0;order<2;order++) {
        char *args[7]={"server",NULL,NULL,NULL,NULL,NULL,NULL};
        int n=1;
        if(order==0) args[n++]="-wslmanaged";
        args[n++]=(char *)conflicts[j][0];
        if(conflicts[j][1]) args[n++]=(char *)conflicts[j][1];
        if(order==1) args[n++]="-wslmanaged";
        args[n++]="-auth"; args[n++]="test.auth";
        options=parse(n,args);
        CHECK(winManagedValidate(&options,0,0)!=NULL);
    }
    /* Generic flags retain their old semantics. Values resembling options
     * are not interpreted as option positions. */
    {char *args[]={"server","-ac","-listen","tcp"}; options=parse(4,args);
     CHECK(winManagedValidate(&options,0,1)==NULL);}
    {char *args[]={"server","-wslmanaged","-auth","-ac"}; options=parse(4,args);
     CHECK(winManagedValidate(&options,0,0)==NULL);}
    {char *args[]={"server","-wslmanaged","-wsldistro","name with spaces","-auth","test.auth"};
     options=parse(6,args); CHECK(winManagedValidate(&options,0,0)==NULL);}
    {char *args[]={"server","-wsldistro"}; options=parse(2,args);
     CHECK(winManagedValidate(&options,0,0)!=NULL);}
    {char *args[]={"server","-wslmanaged","-auth",""};options=parse(4,args);
     CHECK(winManagedValidate(&options,0,0)!=NULL);}
    CHECK(winManagedAuthorityError("missing-file.auth","1")!=NULL);
    {
        FILE *file=fopen("managed-fixture.auth","wb");
        unsigned char cookie[16]={0,13,10,26,255,128};
        Xauth auth={FamilyWild,0,NULL,1,"1",18,"MIT-MAGIC-COOKIE-1",16,(char *)cookie};
        CHECK(file); CHECK(XauWriteAuth(file,&auth)); CHECK(!fclose(file));
        CHECK(winManagedAuthorityError("managed-fixture.auth","1")==NULL);
        CHECK(winManagedAuthorityError("managed-fixture.auth","2")!=NULL);
        auth.data_length=15; file=fopen("managed-fixture.auth","wb");
        CHECK(file);CHECK(XauWriteAuth(file,&auth));fclose(file);
        CHECK(winManagedAuthorityError("managed-fixture.auth","1")!=NULL);
        file=fopen("managed-fixture.auth","wb");CHECK(file);fclose(file);
        CHECK(winManagedAuthorityError("managed-fixture.auth","1")!=NULL);
        file=fopen("managed-fixture.auth","wb"); CHECK(file); fputs("invalid",file); fclose(file);
        CHECK(winManagedAuthorityError("managed-fixture.auth","1")!=NULL);
        remove("managed-fixture.auth");
    }
    puts("PASS managed profile: authority, both argument orders, conflicts, generic isolation");
    return 0;
}
